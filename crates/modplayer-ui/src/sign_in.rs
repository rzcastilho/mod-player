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

use std::time::Duration;

use egui::{Spinner, Ui, WidgetInfo, WidgetType};
use modplayer_account::{AccountEvent, AccountService, SessionState, SignInNote, UPGRADE_URL};
use modplayer_core::{tr, tr_args};

use crate::settings::account::{identity_text, open_sign_out_modal, show_sign_out_modal};
use crate::theme;
use crate::theme::controls::Variant;
use crate::widgets::controls::button;
use crate::widgets::status::status_message;

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
    /// `(attempt_id, egui input time in seconds)` of the frame `Authorizing`
    /// was first seen for that attempt in this process (030, data-model
    /// §1.2). UI-only; never persisted.
    authorizing_since: Option<(u64, f64)>,
}

/// The pending-view elapsed hint (030 C7): whole seconds below a minute,
/// then whole minutes, pluralised by Fluent.
///
/// ```
/// use modplayer_ui::sign_in::elapsed_hint;
/// assert!(elapsed_hint(1).contains("second"));
/// assert!(elapsed_hint(120).contains("minutes"));
/// ```
pub fn elapsed_hint(secs: u64) -> String {
    if secs < 60 {
        tr_args(
            "signin-waiting-elapsed-seconds",
            &[("count", secs.to_string())],
        )
    } else {
        tr_args(
            "signin-waiting-elapsed-minutes",
            &[("count", (secs / 60).to_string())],
        )
    }
}

impl SignInScreen {
    /// Advance the pending-attempt clock for this frame (data-model §1.2)
    /// and return the whole seconds elapsed for the current attempt.
    fn track_authorizing(&mut self, state: &SessionState, now: f64) -> u64 {
        match state {
            SessionState::Authorizing { attempt_id, .. } => {
                let since = match self.authorizing_since {
                    Some((id, t)) if id == *attempt_id => t,
                    _ => {
                        self.authorizing_since = Some((*attempt_id, now));
                        now
                    }
                };
                (now - since).max(0.0).floor() as u64
            }
            _ => {
                self.authorizing_since = None;
                0
            }
        }
    }
}

impl SignInScreen {
    /// Show the tier-result overlay (`App`'s event mapping calls this;
    /// never called with a Premium result — Premium shows nothing).
    pub fn set_tier_result(&mut self, result: TierResult) {
        self.tier_result = Some(result);
    }
}

/// What the sign-in summary asks `App` to do this frame.
///
/// ```
/// use modplayer_ui::sign_in::SummaryOutcome;
/// assert_eq!(SummaryOutcome::None, SummaryOutcome::None);
/// assert_ne!(SummaryOutcome::Continue, SummaryOutcome::Events(Vec::new()));
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum SummaryOutcome {
    /// Nothing happened.
    None,
    /// Continue to the device check.
    Continue,
    /// "Use a different account" went through the sign-out confirmation;
    /// these are the events it raised (empty while the dialog is open).
    Events(Vec<AccountEvent>),
}

/// The signed-in summary shown when the user goes Back from the first-launch
/// device check (030 C16): who is signed in, Continue, or switch account
/// through the existing sign-out confirmation. Persists nothing.
///
/// ```no_run
/// # use modplayer_ui::sign_in::{show_summary, SummaryOutcome};
/// # fn demo(ui: &mut egui::Ui, account: &mut modplayer_account::AccountService) {
/// match show_summary(ui, account) {
///     SummaryOutcome::Continue => { /* go on to the device check */ }
///     SummaryOutcome::Events(_events) => { /* the sign-out went through */ }
///     SummaryOutcome::None => {}
/// }
/// # }
/// ```
pub fn show_summary(ui: &mut Ui, account: &mut AccountService) -> SummaryOutcome {
    ui.heading(tr("signin-summary-title"));
    let name = account.account().map(|a| a.display_name.as_str());
    let identity = identity_text(name.unwrap_or_default());
    prose(
        ui,
        &tr_args("signin-summary-identity", &[("name", identity)]),
    );
    let mut outcome = SummaryOutcome::None;
    ui.horizontal(|ui| {
        if button(ui, Variant::Primary, tr("signin-summary-continue")).clicked() {
            outcome = SummaryOutcome::Continue;
        }
        if button(ui, Variant::Default, tr("signin-summary-switch")).clicked() {
            open_sign_out_modal(ui);
        }
    });
    let events = show_sign_out_modal(ui, account);
    if !events.is_empty() {
        return SummaryOutcome::Events(events);
    }
    outcome
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
    let now = ui.ctx().input(|i| i.time);
    let elapsed = screen.track_authorizing(account.state(), now);

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
            show_authorizing(ui, account, resumed, elapsed)
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
    if let Some(note) = note {
        status_message(ui, &tr(note_key(note)));
    }
    prose(ui, &tr("signin-explanation"));
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
    // 030 C9: the explanation and its remedy form one status message.
    let message = format!(
        "{} {}",
        tr_args("signin-store-unavailable", &[("store", tr(store_name_key))],),
        tr(remedy_key(store_name_key))
    );
    status_message(ui, &message);
}

/// A prose block capped at the 72-character measure (FR-006, research R17).
fn prose(ui: &mut Ui, text: &str) {
    ui.scope(|ui| {
        ui.set_max_width(ui.available_width().min(theme::body_measure(ui.ctx())));
        ui.label(text);
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

fn show_authorizing(
    ui: &mut Ui,
    account: &mut AccountService,
    resumed: bool,
    elapsed: u64,
) -> Vec<AccountEvent> {
    ui.heading(tr("signin-waiting"));
    let indicator = tr("signin-waiting-indicator");
    let spinner = ui.add(Spinner::new());
    spinner.widget_info(|| {
        WidgetInfo::labeled(WidgetType::ProgressIndicator, true, indicator.clone())
    });
    prose(ui, &tr("signin-browser-hint"));
    prose(ui, &elapsed_hint(elapsed));
    if resumed {
        prose(ui, &tr("signin-waiting-resumed"));
    }
    // The elapsed text must advance with no input (030 R3).
    ui.ctx().request_repaint_after(Duration::from_secs(1));

    let mut events = Vec::new();
    ui.horizontal(|ui| {
        if button(ui, Variant::Primary, tr("signin-open-again")).clicked()
            && let Some(url) = account.browser_url()
        {
            events.push(AccountEvent::BrowserUrlReady(url));
        }
        if button(ui, Variant::Default, tr("signin-cancel")).clicked() {
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
            status_message(ui, &tr("tier-free-explanation"));
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
            status_message(ui, &tr("tier-unknown-explanation"));
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

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::disallowed_methods)]
mod tests {
    use std::sync::Arc;

    use egui::accesskit::Role;
    use modplayer_account::{Clock, FakeAuthorizationService, FakeClock};
    use modplayer_secure_store::{MemorySecureStore, SecureStore};

    use super::*;

    fn account() -> AccountService {
        AccountService::new(
            Arc::new(MemorySecureStore::new()) as Arc<dyn SecureStore>,
            Arc::new(FakeAuthorizationService::new())
                as Arc<dyn modplayer_account::AuthorizationService>,
            Arc::new(FakeClock::default()) as Arc<dyn Clock>,
            std::env::temp_dir().join(format!("modplayer-ui-030-signin-{}", std::process::id())),
        )
    }

    /// Run one frame and return the labels of every `Role::Status` node.
    fn statuses(render: impl FnMut(&mut Ui)) -> Vec<String> {
        let ctx = egui::Context::default();
        theme::apply_tokens(&ctx);
        ctx.enable_accesskit();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(960.0, 640.0),
            )),
            ..Default::default()
        };
        let mut render = render;
        let mut output = ctx.run_ui(input, |ui| render(ui));
        let update = output.platform_output.accesskit_update.take().expect("ak");
        output.drop_without_applying_deltas();
        update
            .nodes
            .iter()
            .filter(|(_, n)| n.role() == Role::Status)
            .filter_map(|(_, n)| n.label().map(str::to_string))
            .collect()
    }

    /// Strip Fluent's bidi isolation marks around placeables.
    fn plain(text: String) -> String {
        text.replace(['\u{2068}', '\u{2069}'], "")
    }

    #[test]
    fn elapsed_hint_boundaries() {
        assert_eq!(plain(elapsed_hint(0)), "Waiting for 0 seconds");
        assert_eq!(plain(elapsed_hint(1)), "Waiting for 1 second");
        assert_eq!(plain(elapsed_hint(59)), "Waiting for 59 seconds");
        assert_eq!(plain(elapsed_hint(60)), "Waiting for 1 minute");
        assert_eq!(plain(elapsed_hint(119)), "Waiting for 1 minute");
        assert_eq!(plain(elapsed_hint(120)), "Waiting for 2 minutes");
    }

    #[test]
    fn every_signed_out_note_is_one_status_message() {
        for note in [
            SignInNote::Cancelled,
            SignInNote::TimedOut,
            SignInNote::ServiceError,
            SignInNote::PreviousDidNotFinish,
            SignInNote::Revoked,
        ] {
            let mut account = account();
            let mut screen = SignInScreen::default();
            let found = statuses(|ui| {
                let _ = show_signed_out(ui, &mut account, &mut screen, Some(note));
            });
            assert_eq!(found, vec![tr(note_key(note))], "{note:?}");
        }
    }

    #[test]
    fn no_reason_renders_no_status() {
        let mut account = account();
        let mut screen = SignInScreen::default();
        let none = statuses(|ui| {
            let _ = show_signed_out(ui, &mut account, &mut screen, None);
        });
        assert!(none.is_empty());
        let checking = statuses(show_checking);
        assert!(checking.is_empty());
    }

    #[test]
    fn store_unreadable_is_one_combined_status() {
        let mut account = account();
        let found = statuses(|ui| {
            let _ = show_store_unreadable(ui, &mut account);
        });
        assert_eq!(found.len(), 1);
        let store = tr(account.secure().platform_name_key());
        assert!(found[0].contains(&store));
        assert!(found[0].contains(&tr(remedy_key(account.secure().platform_name_key()))));
    }

    #[test]
    fn authorizing_clock_resets_per_attempt() {
        let mut screen = SignInScreen::default();
        let a = SessionState::Authorizing {
            attempt_id: 1,
            resumed: false,
        };
        assert_eq!(screen.track_authorizing(&a, 10.0), 0);
        assert_eq!(screen.track_authorizing(&a, 75.5), 65);
        // Clock going backwards clamps to zero.
        assert_eq!(screen.track_authorizing(&a, 5.0), 0);
        let b = SessionState::Authorizing {
            attempt_id: 2,
            resumed: false,
        };
        assert_eq!(screen.track_authorizing(&b, 80.0), 0);
        let out = SessionState::SignedOut { note: None };
        assert_eq!(screen.track_authorizing(&out, 90.0), 0);
        assert_eq!(screen.track_authorizing(&b, 100.0), 0, "re-entry restarts");
    }
}

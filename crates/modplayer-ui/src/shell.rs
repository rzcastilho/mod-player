// SPDX-License-Identifier: MIT OR Apache-2.0

//! The app shell's left-rail navigation (contracts/ui-surface.md "Main
//! window"): five sections — Library, Search, Now Playing, Plugins,
//! Settings — selectable by pointer or `Ctrl/Cmd+1..5`. Drawing the five
//! nav buttons in this fixed order also fixes their Tab focus order to
//! match (egui's default focus order follows widget-creation order within
//! a frame). `app.rs` owns the `CentralPanel` that renders whichever
//! section is selected; this module only owns the rail and the Plugins
//! placeholder, which has no dedicated screen yet (Constitution Principle
//! II). The Library placeholder wiring point that used to live here is
//! dropped (004-search-and-library-browse, T026) — `app.rs` renders that
//! section's content directly until `library_view` (US2, T063/T065)
//! replaces it.
//!
//! `Section::Search` is new (004-search-and-library-browse,
//! contracts/ui-surface.md §1): `Ctrl/Cmd+F` or `/` from anywhere jumps to
//! it and asks the search box (`search_view.rs`, US1, T038/T039) to take
//! focus, via `focus_search_requested`.

use egui::{Order, Ui};
use modplayer_core::tr;

/// 011-plugin-ui-contributions, contracts/ui-panels.md L2: a floated
/// plugin panel is a plain `Order::Middle` window — the same layer the
/// notification area's own `Area` uses — so it never floats above a
/// critical notification by construction. `plugin_panels.rs`'s floated
/// `Window`s are built with this constant rather than a bare
/// `egui::Order::Middle` literal, so the ordering *policy* (which layer
/// a plugin surface may use) stays declared in one place, next to the
/// section it only ever appears in.
///
/// Known caveat (2026-09-20 manual walk, recorded in this feature's
/// Manual Scenario Log): `app.rs` currently draws the notification
/// `Area` *before* `CentralPanel`/`now_playing::show`, so within-layer
/// insertion order still lets a freshly (re)drawn floated panel paint
/// over a notification this same frame. Fixing that requires reordering
/// `app.rs`'s own draw calls, which is out of this phase's file scope;
/// tracked for a follow-up rather than silently left undocumented.
pub const PLUGIN_FLOATED_WINDOW_ORDER: Order = Order::Middle;

/// The five navigable sections (contracts/ui-surface.md §1), in the fixed
/// left-rail display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Library,
    Search,
    NowPlaying,
    Plugins,
    Settings,
}

/// Every section with its Fluent label key, in nav-rail display order.
/// `nav-search` lands in `locales/en-US/app.ftl` with the rest of Search
/// (US1, T040); until then it renders as the raw key (`tr`'s missing-key
/// fallback).
const SECTIONS: [(Section, &str); 5] = [
    (Section::Library, "nav-library"),
    (Section::Search, "nav-search"),
    (Section::NowPlaying, "nav-now-playing"),
    (Section::Plugins, "nav-plugins"),
    (Section::Settings, "nav-settings"),
];

/// The shell's own UI state: which section is currently selected, and
/// whether a global shortcut just asked the search box to take focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shell {
    pub section: Section,
    /// Set by `actions::invoke`'s `FocusSearch` arm (007, contracts/
    /// ui-actions.md §3; formerly `Shell::handle_shortcuts`'s own
    /// `Ctrl/Cmd+F`/`/` handling) — `search_view::show` consumes and
    /// clears it to request keyboard focus on the search box the same
    /// frame.
    pub focus_search_requested: bool,
}

impl Default for Shell {
    fn default() -> Self {
        Self {
            section: Section::Library,
            focus_search_requested: false,
        }
    }
}

impl Shell {
    /// Draw the left-rail nav buttons in fixed order, updating `section` on
    /// click. Each button's visible text is also its accessible name
    /// (egui sets both from the same string automatically).
    pub fn nav_rail(&mut self, ui: &mut Ui) {
        for (section, key) in SECTIONS {
            if ui
                .selectable_label(self.section == section, tr(key))
                .clicked()
            {
                self.section = section;
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;
    use egui::accesskit::Role;
    use egui::{Context, RawInput, Ui};
    use modplayer_core::{NotificationCenter, Severity};

    #[test]
    fn default_section_is_library() {
        assert_eq!(Shell::default().section, Section::Library);
    }

    /// Every shell/nav/notification widget exposes an explicit accessible
    /// name (research.md R9's accessibility caveat, FR-022): render the nav
    /// rail, the placeholders, and one notification of each severity, then
    /// walk the AccessKit tree `egui` produces and assert every clickable
    /// (`Role::Button`) node — every nav button, every Dismiss button — has
    /// a non-empty label.
    #[test]
    fn every_shell_and_notification_widget_has_an_accessible_name() {
        let ctx = Context::default();
        ctx.enable_accesskit();

        let mut shell = Shell::default();
        let mut center = NotificationCenter::new();
        center.raise(Severity::Critical, "sample-notification-critical");
        center.raise(Severity::Warning, "sample-notification-warning");
        center.raise(Severity::Info, "sample-notification-info");

        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            shell.nav_rail(ui);
            crate::notifications::show(ui, &center);
        });

        let Some(update) = output.platform_output.accesskit_update.take() else {
            panic!("accesskit_update should be populated once enabled");
        };
        // The font atlas texture delta from this synthetic frame is never
        // painted by anything; tell egui we're intentionally discarding it
        // rather than triggering its debug-mode "unapplied deltas" panic.
        output.drop_without_applying_deltas();

        let mut buttons_checked = 0;
        for (_, node) in &update.nodes {
            if node.role() == Role::Button {
                buttons_checked += 1;
                assert!(
                    node.label().is_some_and(|label| !label.is_empty()),
                    "a Button-role node has no accessible name"
                );
            }
        }
        // 5 nav buttons + 3 Dismiss buttons (one per notification raised above).
        assert_eq!(buttons_checked, 8);
    }

    /// Run `render` in a fresh headless context with AccessKit enabled,
    /// and assert every `Button`/`ProgressIndicator`-role node it produces
    /// has a non-empty accessible name (research.md R9, FR-023;
    /// Constitution X). A fresh `Context` per call avoids widget-id reuse
    /// across the different screen configurations this test exercises.
    fn accessible_widget_count(render: impl FnMut(&mut Ui)) -> usize {
        let ctx = Context::default();
        ctx.enable_accesskit();
        let mut output = ctx.run_ui(RawInput::default(), render);
        let Some(update) = output.platform_output.accesskit_update.take() else {
            panic!("accesskit_update should be populated once enabled");
        };
        output.drop_without_applying_deltas();

        let mut checked = 0;
        for (_, node) in &update.nodes {
            if node.role() == Role::Button || node.role() == Role::ProgressIndicator {
                checked += 1;
                assert!(
                    node.label().is_some_and(|label| !label.is_empty()),
                    "a {:?}-role node has no accessible name",
                    node.role()
                );
            }
        }
        checked
    }

    fn fresh_account_service() -> modplayer_account::AccountService {
        use modplayer_account::{AuthorizationService, Clock, FakeAuthorizationService, FakeClock};
        use modplayer_secure_store::{MemorySecureStore, SecureStore};
        use std::sync::Arc;

        modplayer_account::AccountService::new(
            Arc::new(MemorySecureStore::new()) as Arc<dyn SecureStore>,
            Arc::new(FakeAuthorizationService::new()) as Arc<dyn AuthorizationService>,
            Arc::new(FakeClock::default()) as Arc<dyn Clock>,
            std::env::temp_dir().join(format!(
                "modplayer-ui-shell-a11y-test-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::SystemTime::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or_default()
            )),
        )
    }

    /// Drive a real sign-in to `Active`/Premium via a real loopback
    /// listener and `FakeAuthorizationService`, exactly like
    /// `modplayer-account`'s own US2 integration tests — needed to reach
    /// Settings › Account's signed-in view, which only renders once
    /// `state()` is `Active`/`Expired`.
    fn active_account_service() -> modplayer_account::AccountService {
        use modplayer_account::auth_service::REDIRECT_PATH;
        use modplayer_account::fake_auth::ScriptedCall;
        use modplayer_account::{
            AccountEvent, AuthorizationService, Clock, FakeAuthorizationService, FakeClock,
            Profile, Tier, TokenSet,
        };
        use modplayer_secure_store::{MemorySecureStore, SecureStore};
        use std::io::Write;
        use std::net::TcpStream;
        use std::sync::Arc;
        use std::time::Duration;

        let auth = FakeAuthorizationService::new();
        auth.push_exchange_code(ScriptedCall::ok(TokenSet {
            access_token: "access-token".to_string(),
            refresh_token: Some("refresh-token".to_string()),
            expires_in: Duration::from_secs(3600),
            scope: "streaming".to_string(),
        }));
        auth.push_fetch_profile(ScriptedCall::ok(Profile {
            id: "user-1".to_string(),
            display_name: Some("Alex".to_string()),
            tier: Tier::Premium,
        }));

        let mut service = modplayer_account::AccountService::new(
            Arc::new(MemorySecureStore::new()) as Arc<dyn SecureStore>,
            Arc::new(auth) as Arc<dyn AuthorizationService>,
            Arc::new(FakeClock::default()) as Arc<dyn Clock>,
            std::env::temp_dir().join(format!(
                "modplayer-ui-shell-a11y-active-test-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::SystemTime::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or_default()
            )),
        );

        let events = service.start_sign_in();
        let Some(AccountEvent::BrowserUrlReady(url)) = events.into_iter().next() else {
            panic!("expected BrowserUrlReady");
        };
        let query = url.split('?').nth(1).expect("query string");
        let mut port = None;
        let mut state = None;
        for pair in query.split('&') {
            let (k, v) = pair.split_once('=').expect("key=value");
            match k {
                "port" => port = Some(v.to_string()),
                "state" => state = Some(v.to_string()),
                _ => {}
            }
        }
        let (port, state) = (port.expect("port"), state.expect("state"));

        let addr = format!("127.0.0.1:{port}");
        let mut connected = false;
        for _ in 0..50 {
            if let Ok(mut stream) = TcpStream::connect(&addr) {
                let _ = stream.write_all(
                    format!("GET {REDIRECT_PATH}?code=auth-code&state={state} HTTP/1.1\r\n\r\n")
                        .as_bytes(),
                );
                connected = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(connected, "could not connect to loopback listener");

        for _ in 0..200 {
            service.tick();
            if matches!(service.state(), modplayer_account::SessionState::Active) {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            matches!(service.state(), modplayer_account::SessionState::Active),
            "test setup: expected Active, got {:?}",
            service.state()
        );
        service
    }

    /// Every account/disclosure widget exposes an explicit accessible name
    /// (contracts/ui-surface.md; FR-023; Constitution X): the sign-in
    /// step's SignedOut/Authorizing/tier-result sub-views, Settings ›
    /// Account's signed-in/signed-out views and sign-out confirmation
    /// modal, Settings › About's main/privacy sub-views, and the Welcome
    /// (first-launch disclosure) screen — T106's final accessibility
    /// sweep across every US1-US4 surface.
    #[test]
    fn every_account_widget_has_an_accessible_name() {
        use crate::settings::about::{self, AboutScreen};
        use crate::sign_in::{self, SignInScreen, TierResult};

        // SignedOut, no note: one "Sign in" button.
        let mut signed_out = fresh_account_service();
        let mut signed_out_screen = SignInScreen::default();
        assert_eq!(
            accessible_widget_count(|ui| {
                sign_in::show(ui, &mut signed_out, &mut signed_out_screen);
            }),
            1
        );

        // Authorizing: "Open the browser again" + "Cancel".
        let mut authorizing = fresh_account_service();
        authorizing.start_sign_in();
        let mut authorizing_screen = SignInScreen::default();
        assert_eq!(
            accessible_widget_count(|ui| {
                sign_in::show(ui, &mut authorizing, &mut authorizing_screen);
            }),
            2
        );

        // Tier result: Free — "Open upgrade page" + "Continue".
        let mut tier_service = fresh_account_service();
        let mut free_screen = SignInScreen::default();
        free_screen.set_tier_result(TierResult::Free);
        assert_eq!(
            accessible_widget_count(|ui| {
                sign_in::show(ui, &mut tier_service, &mut free_screen);
            }),
            2
        );

        // Tier result: Unknown — "Retry" + "Continue".
        let mut unknown_screen = SignInScreen::default();
        unknown_screen.set_tier_result(TierResult::Unknown);
        assert_eq!(
            accessible_widget_count(|ui| {
                sign_in::show(ui, &mut tier_service, &mut unknown_screen);
            }),
            2
        );

        // Settings > About, main sub-view: "Privacy notice" + "Disclosure".
        let mut about_screen = AboutScreen::default();
        assert_eq!(
            accessible_widget_count(|ui| about::show(ui, &mut about_screen)),
            2
        );

        // The Privacy sub-view's "Back" button — the exact same function
        // About's own Privacy branch calls.
        assert_eq!(
            accessible_widget_count(|ui| {
                let _ = crate::privacy_notice::show(ui);
            }),
            1
        );

        // Settings > Account, signed-in view: "Re-check subscription" +
        // "Sign out" (US3, T085/T086 — the sign-out confirmation modal
        // itself only renders once "Sign out" is clicked, so it adds no
        // widgets to this closed-state count).
        let mut active = active_account_service();
        assert_eq!(
            accessible_widget_count(|ui| {
                let _ = crate::settings::account::show(ui, &mut active);
            }),
            2
        );

        // Settings > Account, signed-out view (US3, T087): "Sign in".
        let mut signed_out_account = fresh_account_service();
        assert_eq!(
            accessible_widget_count(|ui| {
                let _ = crate::settings::account::show(ui, &mut signed_out_account);
            }),
            1
        );

        // Settings > Account, sign-out confirmation modal, open (US3,
        // T085): "Cancel" + destructive "Sign out".
        let mut modal_account = fresh_account_service();
        assert_eq!(
            accessible_widget_count(|ui| {
                let _ =
                    crate::settings::account::show_sign_out_modal_for_test(ui, &mut modal_account);
            }),
            2
        );

        // Welcome, disclosure sub-view (US1, T039/T041): "Privacy notice"
        // + "I understand, continue" + "Decline".
        let mut welcome_controller = fresh_playback_controller();
        let mut welcome_screen = crate::welcome::WelcomeScreen::default();
        assert_eq!(
            accessible_widget_count(|ui| {
                welcome_screen.show(ui, &mut welcome_controller);
            }),
            3
        );
    }

    /// A `PlaybackController` over a silent `FakeBackend`, pointed at a
    /// fresh temp `settings.toml` — enough to draw `WelcomeScreen` without
    /// touching real audio devices (mirrors `settings/developer.rs`'s own
    /// test fixture).
    fn fresh_playback_controller() -> modplayer_core::PlaybackController<
        modplayer_audio_io::FakeBackend,
        modplayer_audio_source_synthetic::SyntheticHost,
    > {
        use modplayer_core::SettingsStore;
        use std::sync::atomic::{AtomicU64, Ordering};

        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-ui-shell-a11y-welcome-test-{}-{}",
            std::process::id(),
            unique
        ));
        let _ = std::fs::create_dir_all(&dir);
        modplayer_core::PlaybackController::new(
            modplayer_audio_io::FakeBackend::new(vec![]),
            modplayer_audio_source_synthetic::SyntheticHost::new(44_100),
            SettingsStore::with_path(dir.join("settings.toml")),
        )
    }
}

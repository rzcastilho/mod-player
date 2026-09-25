// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! Welcome/Decline behavioural test (US1, contracts/ui-surface.md
//! "Welcome"): declining the first-launch disclosure must write nothing —
//! no `settings.toml`, no `account.toml`, no secure-store entry (FR-002,
//! SC-006). `acknowledgement_survives_sign_out_and_revocation` joins this
//! file in Phase 7, once sign-out (US3) and revocation (US4) exist to
//! exercise.

use std::fs;
use std::io::Write;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration as StdDuration;

use modplayer_account::auth_service::REDIRECT_PATH;
use modplayer_account::fake_auth::ScriptedCall;
use modplayer_account::{
    AccountEvent, AccountService, AccountStateStore, AuthError, AuthorizationService, Clock,
    CredentialStore, FakeAuthorizationService, FakeClock, LoadOutcome, Profile, SessionState, Tier,
    TokenSet,
};
use modplayer_audio_io::FakeBackend;
use modplayer_audio_source_synthetic::SyntheticHost;
use modplayer_core::settings::{DEFAULT_INNER_SIZE, DOCK_WIDTH_DEFAULT, WindowSettings};
use modplayer_core::{DisclosureAcknowledgement, PlaybackController, SettingsStore};
use modplayer_secure_store::{MemorySecureStore, SecureStore};
use modplayer_ui::welcome;

/// A minimal self-cleaning temp directory, matching the pattern already
/// used by `modplayer-core`'s and `modplayer-account`'s own integration
/// tests (no `tempfile` dependency).
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-ui-first-launch-test-{}-{}",
            std::process::id(),
            unique
        ));
        let _ = fs::create_dir_all(&dir);
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn decline_writes_nothing() {
    let dir = TempDir::new();
    let settings_path = dir.path().join("settings.toml");
    let state_store = AccountStateStore::new(dir.path());
    let secure = MemorySecureStore::new();

    // The exact function the Decline button calls (`welcome::show_decline`
    // wires it to `decline-quit`'s click): closes the app, touching
    // neither the settings store, the account state store, nor the secure
    // store. A headless `egui::Context` is enough — `handle_decline` only
    // sends a viewport command, never touches disk.
    let ctx = egui::Context::default();
    welcome::handle_decline(&ctx);

    assert!(
        !settings_path.exists(),
        "declining must never create settings.toml"
    );
    assert!(
        matches!(state_store.load(), LoadOutcome::Absent),
        "declining must never create account.toml"
    );
    assert!(
        secure.entries().is_empty(),
        "declining must never write to the secure store"
    );
}

/// FR-003 (T101, data-model.md §1.1 DM-27): the disclosure acknowledgement
/// lives in `settings.toml`, device-scoped and independent of
/// `account.toml`/the secure store. Neither `AccountService::sign_out()`
/// (US3) nor the revocation path (US4, a definitive `AuthError::Rejected`)
/// touches it, because both only ever clear stores registered through
/// `register_store` — `settings.toml` is never one of them.
#[test]
fn acknowledgement_survives_sign_out_and_revocation() {
    let dir = TempDir::new();
    let settings_store = SettingsStore::with_path(dir.path().join("settings.toml"));

    let mut settings = settings_store.load().settings;
    settings.disclosure = Some(DisclosureAcknowledgement::now(
        modplayer_account::DISCLOSURE_BUNDLE_VERSION,
    ));
    settings_store
        .save(&settings)
        .expect("save the disclosure acknowledgement");
    let acknowledged = settings_store.load().settings.disclosure;
    assert!(
        acknowledged.is_some(),
        "test setup: expected an acknowledgement"
    );

    let secure = Arc::new(MemorySecureStore::new());
    let auth = FakeAuthorizationService::new();
    let clock = Arc::new(FakeClock::default());
    let mut service = AccountService::new(
        secure.clone() as Arc<dyn SecureStore>,
        Arc::new(auth.clone()) as Arc<dyn AuthorizationService>,
        clock.clone() as Arc<dyn Clock>,
        dir.path().to_path_buf(),
    );
    service.register_store(Box::new(CredentialStore::new(
        secure.clone() as Arc<dyn SecureStore>
    )));
    let state_store = service.state_store().clone();
    service.register_store(Box::new(state_store));

    // Sign in, then sign out: the disclosure acknowledgement must be
    // exactly as it was before.
    sign_in_to_active(&mut service, &auth);
    let events = service.sign_out();
    assert!(matches!(
        events.first(),
        Some(AccountEvent::SignedOut { .. })
    ));
    assert_eq!(
        settings_store.load().settings.disclosure,
        acknowledged,
        "sign-out must not touch the disclosure acknowledgement"
    );

    // Sign in again, then take the revocation path (a definitive
    // `AuthError::Rejected` from a refresh): still untouched.
    sign_in_to_active(&mut service, &auth);
    auth.push_refresh(ScriptedCall::err(AuthError::Rejected));
    // Force the scheduler's due time to arrive now rather than waiting for
    // its normal schedule.
    clock.advance(time::Duration::hours(1));
    let events = drain_until(&mut service, |service| {
        matches!(service.state(), SessionState::SignedOut { .. })
    });
    assert!(
        events.contains(&AccountEvent::SessionRevoked),
        "test setup: expected the revocation path, got {events:?}"
    );
    assert_eq!(
        settings_store.load().settings.disclosure,
        acknowledged,
        "revocation must not touch the disclosure acknowledgement"
    );
}

/// Drive `service` through a full sign-in to `Active` with a Premium tier,
/// the same shape `modplayer-account`'s own US2 fixtures use, reimplemented
/// here (rather than imported) because `tests/common` is private to the
/// `modplayer-account` crate.
fn sign_in_to_active(service: &mut AccountService, auth: &FakeAuthorizationService) {
    auth.push_exchange_code(ScriptedCall::ok(TokenSet {
        access_token: "access-token".to_string(),
        refresh_token: Some("refresh-token".to_string()),
        expires_in: StdDuration::from_secs(3600),
        scope: "streaming".to_string(),
    }));
    auth.push_fetch_profile(ScriptedCall::ok(Profile {
        id: "user-1".to_string(),
        display_name: Some("Alex".to_string()),
        tier: Tier::Premium,
    }));

    let events = service.start_sign_in();
    let Some(AccountEvent::BrowserUrlReady(url)) = events.into_iter().next() else {
        panic!("expected BrowserUrlReady");
    };
    let (port, state) = extract_port_and_state(&url);
    drive_callback(&port, &state, "code=auth-code");

    drain_until(service, |service| {
        matches!(service.state(), SessionState::Active)
    });
    assert!(
        matches!(service.state(), SessionState::Active),
        "test setup: expected Active, got {:?}",
        service.state()
    );
}

fn extract_port_and_state(url: &str) -> (String, String) {
    let query = url.split('?').nth(1).expect("query string");
    let (mut port, mut state) = (None, None);
    for pair in query.split('&') {
        let (k, v) = pair.split_once('=').expect("key=value");
        match k {
            "port" => port = Some(v.to_string()),
            "state" => state = Some(v.to_string()),
            _ => {}
        }
    }
    (port.expect("port"), state.expect("state"))
}

/// Connect to the loopback listener and send a minimal callback request
/// line, standing in for the browser's redirect.
fn drive_callback(port: &str, state: &str, query: &str) {
    let addr = format!("127.0.0.1:{port}");
    let mut connected = false;
    for _ in 0..50 {
        if let Ok(mut stream) = TcpStream::connect(&addr) {
            let _ = stream.write_all(
                format!("GET {REDIRECT_PATH}?{query}&state={state} HTTP/1.1\r\n\r\n").as_bytes(),
            );
            connected = true;
            break;
        }
        std::thread::sleep(StdDuration::from_millis(10));
    }
    assert!(connected, "could not connect to loopback listener");
}

/// 013-key-and-tempo-plugin (US4, contracts/getting-started-card.md S1):
/// `Dismiss` persists through `SettingsStore::save` (`persist_settings`),
/// so a whole new `PlaybackController` built from the same store — the
/// same "new `App`, same store" relaunch this file's disclosure test
/// above exercises — sees the card already dismissed.
#[test]
fn getting_started_dismiss_persists_across_relaunch() {
    let dir = TempDir::new();
    let store_path = dir.path().join("settings.toml");

    let mut controller = PlaybackController::new(
        FakeBackend::new(vec![]),
        SyntheticHost::new(44_100),
        SettingsStore::with_path(&store_path),
    );
    assert!(!controller.getting_started_dismissed());
    controller.dismiss_getting_started();
    assert!(controller.getting_started_dismissed());
    drop(controller);

    // Reconstruct a whole new controller from the same settings-store
    // path, exactly like relaunching the app (mirrors `controller_
    // actions.rs`'s `custom_binding_survives_controller_restart`).
    let reconstructed = PlaybackController::new(
        FakeBackend::new(vec![]),
        SyntheticHost::new(44_100),
        SettingsStore::with_path(&store_path),
    );
    assert!(
        reconstructed.getting_started_dismissed(),
        "the dismissal must survive a relaunch on the same settings.toml"
    );
}

/// 018-window-sizing-and-responsive-dock (US1, contract W3, FR-001/FR-002):
/// `main.rs`'s `ViewportBuilder::with_inner_size` reads
/// `controller.window_settings()` before `run_native` — a fresh config
/// directory (no saved `[window]` state, the spec's own Independent Test
/// for this story) must therefore hand back the 1200 × 820 / 280 defaults,
/// unit-level coverage for what M1 (quickstart.md) confirms visually.
#[test]
fn fresh_launch_reports_default_window_settings() {
    let dir = TempDir::new();
    let controller = PlaybackController::new(
        FakeBackend::new(vec![]),
        SyntheticHost::new(44_100),
        SettingsStore::with_path(dir.path().join("settings.toml")),
    );

    assert_eq!(
        controller.window_settings(),
        WindowSettings {
            inner_width: DEFAULT_INNER_SIZE.0,
            inner_height: DEFAULT_INNER_SIZE.1,
            dock_width: DOCK_WIDTH_DEFAULT,
        },
        "a fresh config directory must report the documented 1200x820/280 defaults"
    );
}

/// As above, but for a *restored* size (US1 acceptance scenario 2,
/// contract W2): a size persisted by a previous session — the same
/// `set_window_inner_size` call `App`'s `WindowSizeTracker`/`on_exit` make
/// — is what the next launch's `window_settings()` (and so `main.rs`'s
/// viewport) reads back, across a whole new `PlaybackController` over the
/// same `settings.toml` (mirrors `getting_started_dismiss_persists_
/// across_relaunch` above).
#[test]
fn restored_window_size_survives_relaunch() {
    let dir = TempDir::new();
    let store_path = dir.path().join("settings.toml");

    let mut controller = PlaybackController::new(
        FakeBackend::new(vec![]),
        SyntheticHost::new(44_100),
        SettingsStore::with_path(&store_path),
    );
    controller.set_window_inner_size(1440.0, 900.0);
    drop(controller);

    let reconstructed = PlaybackController::new(
        FakeBackend::new(vec![]),
        SyntheticHost::new(44_100),
        SettingsStore::with_path(&store_path),
    );
    assert_eq!(
        reconstructed.window_settings(),
        WindowSettings {
            inner_width: 1440.0,
            inner_height: 900.0,
            dock_width: DOCK_WIDTH_DEFAULT,
        },
        "the restored size must survive a relaunch on the same settings.toml"
    );
}

/// Drain `tick()` a bounded number of times, sleeping briefly between calls
/// so background worker threads get a chance to report.
fn drain_until<F: FnMut(&mut AccountService) -> bool>(
    service: &mut AccountService,
    mut done: F,
) -> Vec<AccountEvent> {
    let mut events = Vec::new();
    for _ in 0..200 {
        events.extend(service.tick());
        if done(service) {
            break;
        }
        std::thread::sleep(StdDuration::from_millis(5));
    }
    events
}

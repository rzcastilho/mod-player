// SPDX-License-Identifier: MIT OR Apache-2.0

//! Shared fixtures for the 028 settings integration tests.

#![allow(dead_code, reason = "each test binary uses a subset")]
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::disallowed_methods)]

use std::io::Write;
use std::net::TcpStream;
use std::sync::Arc;
use std::time::Duration;

use modplayer_account::auth_service::REDIRECT_PATH;
use modplayer_account::fake_auth::ScriptedCall;
use modplayer_account::{
    AccountEvent, AccountService, AuthorizationService, Clock, FakeAuthorizationService, FakeClock,
    Profile, SessionState, Tier, TokenSet,
};
use modplayer_secure_store::{MemorySecureStore, SecureStore};

/// An `AccountService` signed in to `Active` / Premium as "Alex", driven
/// through a real loopback callback against the fake authorization service.
pub fn active_account(label: &str) -> AccountService {
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
    let mut service = AccountService::new(
        Arc::new(MemorySecureStore::new()) as Arc<dyn SecureStore>,
        Arc::new(auth) as Arc<dyn AuthorizationService>,
        Arc::new(FakeClock::default()) as Arc<dyn Clock>,
        std::env::temp_dir().join(format!(
            "modplayer-ui-028-{label}-{}-{}",
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
    let (mut port, mut state) = (None, None);
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
        if matches!(service.state(), SessionState::Active) {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        matches!(service.state(), SessionState::Active),
        "test setup: expected Active, got {:?}",
        service.state()
    );
    service
}

/// An `AccountService` that never signed in (`SignedOut`, no account).
pub fn signed_out_account(label: &str) -> AccountService {
    AccountService::new(
        Arc::new(MemorySecureStore::new()) as Arc<dyn SecureStore>,
        Arc::new(FakeAuthorizationService::new()) as Arc<dyn AuthorizationService>,
        Arc::new(FakeClock::default()) as Arc<dyn Clock>,
        std::env::temp_dir().join(format!(
            "modplayer-ui-028-signed-out-{label}-{}",
            std::process::id()
        )),
    )
}

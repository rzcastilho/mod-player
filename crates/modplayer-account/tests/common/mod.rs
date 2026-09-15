// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(dead_code)]
// Integration-test fixtures, not the crate's own production code — the
// `clippy::disallowed_methods`/`unwrap_used`/`expect_used` gate the crate
// sets in `src/lib.rs` doesn't reach `tests/`, but the workspace-wide
// clippy invocation still lints every test binary; every other test file
// in this crate opts out the same way on its own `#[cfg(test)] mod tests`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! Shared fixtures for `modplayer-account`'s US2 integration tests
//! (`sign_in.rs`, `tier.rs`, `refresh.rs`): a service wired to
//! `MemorySecureStore` + `FakeAuthorizationService` + `FakeClock`, and the
//! small pieces of plumbing needed to drive a real loopback callback from
//! outside the crate (only the public API is visible here, unlike
//! `service.rs`'s own `#[cfg(test)]` module).

use std::io::Write;
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration as StdDuration;

use modplayer_account::auth_service::REDIRECT_PATH;
use modplayer_account::{
    AccountEvent, AccountService, AuthorizationService, Clock, FakeAuthorizationService, FakeClock,
    Profile, Tier, TokenSet,
};
use modplayer_secure_store::{MemorySecureStore, SecureStore};

pub fn fresh_dir(label: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "modplayer-account-{label}-test-{}-{}",
        std::process::id(),
        unique
    ))
}

pub struct Fixture {
    pub service: AccountService,
    pub auth: FakeAuthorizationService,
    pub secure: Arc<MemorySecureStore>,
    pub clock: Arc<FakeClock>,
}

pub fn fresh_fixture(label: &str) -> Fixture {
    let secure = Arc::new(MemorySecureStore::new());
    let auth = FakeAuthorizationService::new();
    let clock = Arc::new(FakeClock::default());
    let service = AccountService::new(
        secure.clone() as Arc<dyn SecureStore>,
        Arc::new(auth.clone()) as Arc<dyn AuthorizationService>,
        clock.clone() as Arc<dyn Clock>,
        fresh_dir(label),
    );
    Fixture {
        service,
        auth,
        secure,
        clock,
    }
}

/// Construct a fresh `AccountService` around already-shared collaborators
/// (US4's `launch.rs` relaunch tests: two service "generations" pointed at
/// the same secure store and `account.toml` directory, standing in for
/// "the app was closed and relaunched").
pub fn service_with(
    secure: Arc<MemorySecureStore>,
    auth: &FakeAuthorizationService,
    clock: Arc<FakeClock>,
    dir: PathBuf,
) -> AccountService {
    AccountService::new(
        secure as Arc<dyn SecureStore>,
        Arc::new(auth.clone()) as Arc<dyn AuthorizationService>,
        clock as Arc<dyn Clock>,
        dir,
    )
}

pub fn token_set() -> TokenSet {
    TokenSet {
        access_token: "access-token".to_string(),
        refresh_token: Some("refresh-token".to_string()),
        expires_in: StdDuration::from_secs(3600),
        scope: "streaming".to_string(),
    }
}

pub fn profile(tier: Tier) -> Profile {
    Profile {
        id: "user-1".to_string(),
        display_name: Some("Alex".to_string()),
        tier,
    }
}

/// Extract `key`'s value from a `?a=b&c=d`-style URL —
/// `FakeAuthorizationService::authorization_url` embeds `state`/`port`
/// directly for exactly this purpose.
pub fn extract_query_value(url: &str, key: &str) -> Option<String> {
    let query = url.split('?').nth(1)?;
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == key).then(|| v.to_string())
    })
}

/// Connect to the loopback listener on `port` and send a minimal
/// `GET {REDIRECT_PATH}?{query}&state={state}` request line, standing in
/// for the browser's redirect (contracts/authorization-service.md
/// "Loopback callback HTTP contract").
pub fn drive_callback(port: &str, state: &str, query: &str) {
    let addr = format!("127.0.0.1:{port}");
    // The listener thread starts polling immediately, but a freshly
    // spawned OS thread may not have called `accept()` yet; retry the
    // connect briefly rather than making these tests flaky under load.
    let mut last_err = None;
    for _ in 0..50 {
        match TcpStream::connect(&addr) {
            Ok(mut stream) => {
                let _ = stream.write_all(
                    format!("GET {REDIRECT_PATH}?{query}&state={state} HTTP/1.1\r\n\r\n")
                        .as_bytes(),
                );
                return;
            }
            Err(err) => {
                last_err = Some(err);
                std::thread::sleep(StdDuration::from_millis(10));
            }
        }
    }
    panic!("could not connect to loopback listener on {addr}: {last_err:?}");
}

/// Drain `tick()` a bounded number of times, sleeping briefly between
/// calls so background worker threads (real OS threads, not driven by the
/// fake clock for the exchange/tier hops, which have zero scripted delay)
/// get a chance to report.
pub fn drain_until<F: FnMut(&mut AccountService) -> bool>(
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

/// Start sign-in and return the `port`/`state` pair a browser would carry
/// in its redirect, panicking if `start_sign_in` didn't produce
/// `BrowserUrlReady` (every US2 happy-path test begins this way).
pub fn start_sign_in_and_extract_callback_target(service: &mut AccountService) -> (String, String) {
    let events = service.start_sign_in();
    let Some(AccountEvent::BrowserUrlReady(url)) = events.into_iter().next() else {
        panic!("expected BrowserUrlReady");
    };
    let port = extract_query_value(&url, "port").expect("port in fake url");
    let state = extract_query_value(&url, "state").expect("state in fake url");
    (port, state)
}

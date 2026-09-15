// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! FR-020 / SC-004: runs the full sign-in flow with a distinctive token
//! value, then greps every surface *other* than the secure store —
//! `account.toml` and every other file under the config directory, the
//! notification center, and the `Debug` output of every public value
//! `AccountService` exposes — for the token bytes. This codebase has no
//! logging framework (nothing to capture there); the equivalent surfaces
//! it does have are covered instead.

use std::fs;
use std::io::Write;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use modplayer_account::auth_service::REDIRECT_PATH;
use modplayer_account::fake_auth::ScriptedCall;
use modplayer_account::{
    AccountEvent, AccountService, AuthorizationService, Clock, FakeAuthorizationService, FakeClock,
    Profile, SessionState, Tier, TokenSet,
};
use modplayer_core::{NotificationCenter, Severity};
use modplayer_secure_store::{MemorySecureStore, SecureStore};

/// A value distinctive enough that finding it anywhere other than the
/// secure store is unambiguous evidence of a leak.
const MARKER_ACCESS_TOKEN: &str = "MARKER-ACCESS-TOKEN-df8f21c6";
const MARKER_REFRESH_TOKEN: &str = "MARKER-REFRESH-TOKEN-9b7a10e4";

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-ui-credential-leak-test-{}-{}",
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
fn credential_never_appears_outside_secure_store() {
    let dir = TempDir::new();
    let secure = Arc::new(MemorySecureStore::new());
    let auth = FakeAuthorizationService::new();
    auth.push_exchange_code(ScriptedCall::ok(TokenSet {
        access_token: MARKER_ACCESS_TOKEN.to_string(),
        refresh_token: Some(MARKER_REFRESH_TOKEN.to_string()),
        expires_in: Duration::from_secs(3600),
        scope: "streaming".to_string(),
    }));
    auth.push_fetch_profile(ScriptedCall::ok(Profile {
        id: "user-1".to_string(),
        display_name: Some("Alex".to_string()),
        tier: Tier::Premium,
    }));

    let mut service = AccountService::new(
        secure.clone() as Arc<dyn SecureStore>,
        Arc::new(auth) as Arc<dyn AuthorizationService>,
        Arc::new(FakeClock::default()) as Arc<dyn Clock>,
        dir.path().to_path_buf(),
    );

    let events = service.start_sign_in();
    let Some(AccountEvent::BrowserUrlReady(url)) = events.into_iter().next() else {
        panic!("expected BrowserUrlReady");
    };
    assert!(
        !url.contains(MARKER_ACCESS_TOKEN) && !url.contains(MARKER_REFRESH_TOKEN),
        "the authorization URL must never carry the token"
    );

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

    let mut all_events = Vec::new();
    for _ in 0..200 {
        all_events.extend(service.tick());
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

    // The secure store legitimately holds the token — that is FR-009's
    // whole point — so its own payload is excluded from the scan.
    let secure_store_bytes: Vec<u8> = secure
        .entries()
        .values()
        .flat_map(|bytes| bytes.iter().copied())
        .collect();
    assert!(String::from_utf8_lossy(&secure_store_bytes).contains(MARKER_ACCESS_TOKEN));

    // Surface 1: every file under the config directory (`account.toml` and
    // anything else `AccountService` wrote there).
    for entry in walk_files(dir.path()) {
        let content = fs::read(&entry).unwrap_or_default();
        assert_leak_free(&content, &format!("file {}", entry.display()));
    }

    // Surface 2: the notification center — every message/arg text a
    // notification could render, using every event this flow actually
    // raised, mapped the same way `app.rs` would.
    let mut notifications = NotificationCenter::new();
    for event in &all_events {
        if let AccountEvent::RefreshFailing = event {
            notifications.raise(Severity::Warning, "signin-again");
        }
    }
    for notification in notifications.all() {
        assert_leak_free(
            notification.message_key.as_bytes(),
            "notification message key",
        );
        for (_, value) in &notification.args {
            assert_leak_free(value.as_bytes(), "notification arg");
        }
    }

    // Surface 3: `Debug` output of every public value `AccountService`
    // exposes — `SessionCredential`/`TokenSet`/`Profile`'s redacting
    // `Debug` impls are unit-tested in `modplayer-account` directly; this
    // is the end-to-end check that nothing *else* on this path leaks it.
    assert_leak_free(format!("{:?}", service.state()).as_bytes(), "state Debug");
    assert_leak_free(
        format!("{:?}", service.account()).as_bytes(),
        "account Debug",
    );
    assert_leak_free(
        format!("{all_events:?}").as_bytes(),
        "AccountEvent history Debug",
    );

    // `account.toml` itself: only the secure-store entry *name*, never the
    // secret, is ever written there (FR-009).
    let account_bytes =
        fs::read(dir.path().join("account.toml")).expect("account.toml should exist");
    let account_text = String::from_utf8_lossy(&account_bytes);
    assert!(account_text.contains("session-credential"));
}

fn assert_leak_free(bytes: &[u8], surface: &str) {
    let text = String::from_utf8_lossy(bytes);
    assert!(
        !text.contains(MARKER_ACCESS_TOKEN),
        "access token leaked into {surface}"
    );
    assert!(
        !text.contains(MARKER_REFRESH_TOKEN),
        "refresh token leaked into {surface}"
    );
}

fn walk_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files.extend(walk_files(&path));
        } else {
            files.push(path);
        }
    }
    files
}

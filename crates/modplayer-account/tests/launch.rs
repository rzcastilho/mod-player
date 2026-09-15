// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! US4 `launch()` integration tests (contracts/account-session.md
//! "Construction"): two generations of `AccountService` sharing the same
//! secure store and `account.toml` directory, standing in for "the app was
//! closed and relaunched."

mod common;

use std::net::TcpListener;
use std::sync::Arc;
use std::time::Duration as StdDuration;

use common::{drain_until, drive_callback, fresh_dir, profile, service_with, token_set};
use modplayer_account::fake_auth::ScriptedCall;
use modplayer_account::{
    AccountEvent, FakeAuthorizationService, FakeClock, SessionState, SignInNote, Tier,
};
use modplayer_secure_store::{EntryName, MemorySecureStore, SecureStore};

/// Repeatedly bind-and-immediately-drop `port` until it succeeds, standing
/// in for "the OS actually freed the port" once the old listener thread's
/// cancel-poll (50 ms, `listener::POLL_INTERVAL`) noticed the flag Drop
/// sets and exited.
fn wait_for_port_free(port: u16) {
    for _ in 0..100 {
        if let Ok(listener) = TcpListener::bind(("127.0.0.1", port)) {
            drop(listener);
            return;
        }
        std::thread::sleep(StdDuration::from_millis(20));
    }
    panic!("port {port} never became free");
}

/// FR-018 / SC-005 (T089): a pending attempt younger than 5 minutes is
/// resumed (re-bound, `Authorizing { resumed: true }`, and genuinely still
/// completable) on the next launch; one older than 5 minutes is discarded
/// with `PreviousDidNotFinish` instead, even though the port is free to
/// re-bind.
#[test]
fn relaunch_resumes_young_attempt_and_discards_old() {
    let secure = Arc::new(MemorySecureStore::new());
    let auth = FakeAuthorizationService::new();
    let clock = Arc::new(FakeClock::default());
    let dir = fresh_dir("relaunch-resume");

    let mut first = service_with(secure.clone(), &auth, clock.clone(), dir.clone());
    let events = first.start_sign_in();
    let Some(AccountEvent::BrowserUrlReady(url)) = events.into_iter().next() else {
        panic!("expected BrowserUrlReady");
    };
    let port: u16 = common::extract_query_value(&url, "port")
        .expect("port in fake url")
        .parse()
        .expect("numeric port");
    let state = common::extract_query_value(&url, "state").expect("state in fake url");

    // "App closed": dropping the service signals the listener's cancel
    // flag (`AccountService`'s `Drop` impl); wait for the OS to actually
    // free the port before the next generation tries to re-bind it.
    drop(first);
    wait_for_port_free(port);

    let mut second = service_with(secure.clone(), &auth, clock.clone(), dir.clone());
    let outcome = second.launch();
    assert!(
        matches!(
            outcome.state,
            SessionState::Authorizing { resumed: true, .. }
        ),
        "expected a resumed Authorizing state, got {:?}",
        outcome.state
    );
    assert_eq!(second.state(), &outcome.state);

    // The resumed listener genuinely works: driving the callback through
    // it completes sign-in exactly like a fresh attempt would.
    auth.push_exchange_code(ScriptedCall::ok(token_set()));
    auth.push_fetch_profile(ScriptedCall::ok(profile(Tier::Premium)));
    drive_callback(&port.to_string(), &state, "code=auth-code");
    drain_until(&mut second, |service| {
        matches!(service.state(), SessionState::Active)
    });
    assert_eq!(second.tier(), Tier::Premium);
    drop(second);

    // A pending attempt older than 5 minutes is discarded instead of
    // resumed.
    let secure_old = Arc::new(MemorySecureStore::new());
    let auth_old = FakeAuthorizationService::new();
    let clock_old = Arc::new(FakeClock::default());
    let dir_old = fresh_dir("relaunch-discard-old");

    let mut stale = service_with(
        secure_old.clone(),
        &auth_old,
        clock_old.clone(),
        dir_old.clone(),
    );
    let events = stale.start_sign_in();
    let Some(AccountEvent::BrowserUrlReady(stale_url)) = events.into_iter().next() else {
        panic!("expected BrowserUrlReady");
    };
    let stale_port: u16 = common::extract_query_value(&stale_url, "port")
        .expect("port")
        .parse()
        .expect("numeric port");
    drop(stale);
    wait_for_port_free(stale_port);

    clock_old.advance(time::Duration::minutes(6));
    let mut relaunched = service_with(secure_old.clone(), &auth_old, clock_old, dir_old);
    let outcome = relaunched.launch();
    assert_eq!(
        outcome.state,
        SessionState::SignedOut {
            note: Some(SignInNote::PreviousDidNotFinish)
        }
    );
    assert!(
        !secure_old
            .entries()
            .contains_key(&EntryName::PendingAuthorization),
        "a stale pending attempt must be discarded, not left behind"
    );
}

/// FR-017 (T090): a session found at launch whose secure store fails to
/// read resolves to `StoreUnreadable` and clears nothing — not
/// `account.toml`, not the (unreadable, but still present) credential
/// entry.
#[test]
fn store_unreadable_at_launch_clears_nothing() {
    let secure = Arc::new(MemorySecureStore::new());
    let auth = FakeAuthorizationService::new();
    let clock = Arc::new(FakeClock::default());
    let dir = fresh_dir("store-unreadable-launch");

    // Sign all the way in so a session genuinely exists on disk and in the
    // secure store.
    let mut first = service_with(secure.clone(), &auth, clock.clone(), dir.clone());
    auth.push_exchange_code(ScriptedCall::ok(token_set()));
    auth.push_fetch_profile(ScriptedCall::ok(profile(Tier::Premium)));
    let (port, state) = common::start_sign_in_and_extract_callback_target(&mut first);
    drive_callback(&port, &state, "code=auth-code");
    drain_until(&mut first, |service| {
        matches!(service.state(), SessionState::Active)
    });
    drop(first);

    // Relaunch with the secure store locked/unavailable.
    secure.set_unavailable(true);
    let mut second = service_with(secure.clone(), &auth, clock, dir);
    let outcome = second.launch();

    assert_eq!(outcome.state, SessionState::StoreUnreadable);
    assert_eq!(second.state(), &SessionState::StoreUnreadable);
    assert!(
        matches!(
            second.state_store().load(),
            modplayer_account::LoadOutcome::Loaded(_)
        ),
        "a store read failure at launch must never touch account.toml"
    );

    secure.set_unavailable(false);
    assert!(
        secure.get(EntryName::SessionCredential).unwrap().is_some(),
        "a store read failure at launch must never clear the credential"
    );
}

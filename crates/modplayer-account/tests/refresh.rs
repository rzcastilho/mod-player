// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! US2/US4 refresh-scheduler integration tests (contracts/account-
//! session.md "Refresh scheduler rules"/"Revocation path", FR-013/FR-014/
//! FR-019/FR-021).

mod common;

use std::sync::Arc;
use std::time::Duration as StdDuration;

use common::{
    drain_until, drive_callback, fresh_fixture, profile, start_sign_in_and_extract_callback_target,
    token_set,
};
use modplayer_account::fake_auth::ScriptedCall;
use modplayer_account::{
    AccountEvent, AccountService, AuthError, CredentialStore, LoggedCall, SessionState, SignInNote,
    Tier,
};
use modplayer_secure_store::{EntryName, SecureStore};

/// FR-013 / FR-014: once a credential is `Active`, the scheduler refreshes
/// it when due; three consecutive transient failures raise
/// `RefreshFailing` (the sign-in step's "sign in again" warning) exactly
/// once.
#[test]
fn refresh_backoff_and_signin_again_after_3_failures() {
    let mut fixture = fresh_fixture("refresh-backoff");
    // A 6-minute lifetime is under FR-013's 10-minute short-lifetime
    // threshold, so the scheduler is due at half the lifetime (3 minutes)
    // rather than 5 minutes before expiry (which would already be past at
    // authorization time for a token this short-lived).
    let mut short_lived = token_set();
    short_lived.expires_in = StdDuration::from_secs(360);
    fixture
        .auth
        .push_exchange_code(ScriptedCall::ok(short_lived));
    fixture
        .auth
        .push_fetch_profile(ScriptedCall::ok(profile(Tier::Premium)));

    let (port, state) = start_sign_in_and_extract_callback_target(&mut fixture.service);
    drive_callback(&port, &state, "code=auth-code");
    drain_until(&mut fixture.service, |service| {
        matches!(service.state(), SessionState::Active)
    });

    for _ in 0..3 {
        fixture
            .auth
            .push_refresh(ScriptedCall::err(AuthError::Transient));
    }

    // Reach the first due time.
    fixture.clock.advance(time::Duration::minutes(3));

    let mut raised_failing = false;
    for _ in 0..3 {
        let events = tick_and_wait(&mut fixture.service);
        if events.contains(&AccountEvent::RefreshFailing) {
            raised_failing = true;
        }
        // Comfortably past the 60 s backoff cap — safe for every one of
        // the first three attempts' delays (1 s, 2 s, 4 s).
        fixture.clock.advance(time::Duration::seconds(65));
    }

    assert!(
        raised_failing,
        "expected RefreshFailing after the 3rd consecutive transient failure"
    );
    // A refresh failure never discards the still-valid credential.
    assert!(
        fixture
            .secure
            .entries()
            .contains_key(&EntryName::SessionCredential)
    );
}

/// Tick twice with a short real sleep in between: the first tick spawns
/// the refresh worker (a real OS thread; `FakeAuthorizationService`
/// resolves near-instantly but not synchronously with `tick()` itself),
/// the second drains its result.
fn tick_and_wait(service: &mut AccountService) -> Vec<AccountEvent> {
    let mut events = service.tick();
    std::thread::sleep(StdDuration::from_millis(30));
    events.extend(service.tick());
    events
}

/// FR-021 / SC-005 (T091): when no refresh succeeds before `expires_at`,
/// the session moves to `Expired` (the main window stays available) with
/// the credential retained; the scheduler keeps retrying with backoff
/// afterward, and a later success returns to `Active`.
#[test]
fn expiry_without_refresh_enters_expired_and_retains_credential() {
    let mut fixture = fresh_fixture("expiry-without-refresh");
    // A 6-minute lifetime puts the scheduler's first due time 3 minutes in
    // (FR-013's short-lifetime half-life rule), comfortably before the
    // 6-minute expiry this test crosses.
    let mut short_lived = token_set();
    short_lived.expires_in = StdDuration::from_secs(360);
    fixture
        .auth
        .push_exchange_code(ScriptedCall::ok(short_lived));
    fixture
        .auth
        .push_fetch_profile(ScriptedCall::ok(profile(Tier::Premium)));

    let (port, state) = start_sign_in_and_extract_callback_target(&mut fixture.service);
    drive_callback(&port, &state, "code=auth-code");
    drain_until(&mut fixture.service, |service| {
        matches!(service.state(), SessionState::Active)
    });

    // One transient refresh failure, then cross `expires_at` without ever
    // letting a refresh succeed.
    fixture
        .auth
        .push_refresh(ScriptedCall::err(AuthError::Transient));
    fixture.clock.advance(time::Duration::minutes(10));

    // `check_expiry` reaches `Expired` the same tick `tick_refresh` spawns
    // the (fated-to-fail) refresh attempt due at that point; wait for both
    // — the state transition *and* that attempt's worker thread actually
    // reporting back — before scripting the next call, so the next refresh
    // this test scripts can't be raced by the previous one's still-pending
    // result.
    let auth_handle = fixture.auth.clone();
    let events = drain_until(&mut fixture.service, move |service| {
        matches!(service.state(), SessionState::Expired)
            && auth_handle
                .call_log()
                .iter()
                .filter(|call| matches!(call, LoggedCall::Refresh))
                .count()
                >= 1
    });

    assert!(events.contains(&AccountEvent::SessionExpired));
    assert_eq!(fixture.service.state(), &SessionState::Expired);
    // The credential is retained — a failed/absent refresh never discards
    // it.
    assert!(
        fixture
            .secure
            .entries()
            .contains_key(&EntryName::SessionCredential)
    );

    // The scheduler keeps retrying afterward: a later success returns to
    // `Active`.
    fixture.auth.push_refresh(ScriptedCall::ok(token_set()));
    fixture.clock.advance(time::Duration::minutes(1));
    drain_until(&mut fixture.service, |service| {
        matches!(service.state(), SessionState::Active)
    });
    assert_eq!(fixture.service.state(), &SessionState::Active);
}

/// FR-019 / SC-005 (T092): a definitive `invalid_grant` rejection from the
/// token endpoint (surfaced as `AuthError::Rejected`) takes the full
/// revocation path — credential deleted, every registered store cleared,
/// session `SignedOut(Revoked)` — rather than being retried as a
/// transient failure.
#[test]
fn invalid_grant_takes_revocation_path() {
    let mut fixture = fresh_fixture("invalid-grant-revocation");
    fixture
        .service
        .register_store(Box::new(CredentialStore::new(
            fixture.secure.clone() as Arc<dyn SecureStore>
        )));
    let state_store = fixture.service.state_store().clone();
    fixture.service.register_store(Box::new(state_store));

    fixture
        .auth
        .push_exchange_code(ScriptedCall::ok(token_set()));
    fixture
        .auth
        .push_fetch_profile(ScriptedCall::ok(profile(Tier::Premium)));

    let (port, state) = start_sign_in_and_extract_callback_target(&mut fixture.service);
    drive_callback(&port, &state, "code=auth-code");
    drain_until(&mut fixture.service, |service| {
        matches!(service.state(), SessionState::Active)
    });

    fixture
        .auth
        .push_refresh(ScriptedCall::err(AuthError::Rejected));
    // Force the scheduler's due time to arrive now rather than waiting for
    // its normal 5-minute-before-expiry schedule.
    fixture.clock.advance(time::Duration::hours(1));

    let events = drain_until(&mut fixture.service, |service| {
        matches!(service.state(), SessionState::SignedOut { .. })
    });

    assert!(events.contains(&AccountEvent::SessionRevoked));
    assert_eq!(
        fixture.service.state(),
        &SessionState::SignedOut {
            note: Some(SignInNote::Revoked)
        }
    );
    assert_eq!(fixture.service.tier(), Tier::Unknown);
    assert!(!fixture.service.playback_permitted());
    assert!(
        fixture
            .secure
            .get(EntryName::SessionCredential)
            .unwrap()
            .is_none()
    );
}

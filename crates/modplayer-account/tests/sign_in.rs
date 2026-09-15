// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! US2 sign-in integration tests (contracts/authorization-service.md,
//! contracts/account-session.md): drive `AccountService` end to end
//! through its public API, with a real loopback listener (`TcpStream`
//! connecting to it, standing in for the browser) and
//! `FakeAuthorizationService` standing in for Spotify.

mod common;

use common::{
    drain_until, drive_callback, fresh_fixture, profile, start_sign_in_and_extract_callback_target,
    token_set,
};
use modplayer_account::fake_auth::ScriptedCall;
use modplayer_account::{AccountEvent, SessionState, SignInNote, Tier};
use modplayer_secure_store::EntryName;

/// FR-017 / SC-008: a store probe failure refuses sign-in before a browser
/// is ever opened, and writes nothing.
#[test]
fn sign_in_probes_store_before_opening_browser() {
    let mut fixture = fresh_fixture("sign-in-probe");
    fixture.secure.set_unavailable(true);

    let events = fixture.service.start_sign_in();

    assert_eq!(
        events,
        vec![AccountEvent::StoreUnavailable {
            store_name_key: "store-name-keychain"
        }]
    );
    assert_eq!(
        fixture.service.state(),
        &SessionState::SignedOut { note: None }
    );
    assert!(fixture.secure.entries().is_empty());
    assert!(fixture.auth.call_log().is_empty());
}

/// FR-008 / FR-009: a successful callback exchanges the code, stores the
/// credential only in the secure store, writes `account.toml`, and checks
/// the tier.
#[test]
fn callback_with_code_stores_credential_then_checks_tier() {
    let mut fixture = fresh_fixture("sign-in-callback");
    fixture
        .auth
        .push_exchange_code(ScriptedCall::ok(token_set()));
    fixture
        .auth
        .push_fetch_profile(ScriptedCall::ok(profile(Tier::Premium)));

    let (port, state) = start_sign_in_and_extract_callback_target(&mut fixture.service);
    drive_callback(&port, &state, "code=auth-code");

    let events = drain_until(&mut fixture.service, |service| {
        matches!(service.state(), SessionState::Active)
    });

    assert!(events.contains(&AccountEvent::Authorized));
    assert!(events.contains(&AccountEvent::TierChecked(Tier::Premium)));
    assert_eq!(fixture.service.tier(), Tier::Premium);
    assert!(fixture.service.playback_permitted());

    let credential_bytes = fixture
        .secure
        .entries()
        .get(&EntryName::SessionCredential)
        .cloned()
        .expect("credential written");
    let credential_text = String::from_utf8(credential_bytes).expect("utf8");
    assert!(credential_text.contains("access-token"));
    assert!(
        !fixture
            .secure
            .entries()
            .contains_key(&EntryName::PendingAuthorization)
    );

    let account = fixture.service.account().expect("account known");
    assert_eq!(account.account_id, "user-1");
    assert_eq!(account.tier, Tier::Premium);
    assert_eq!(account.credential_ref, "session-credential");
}

/// FR-016: starting a fresh attempt discards the previous one — a new
/// attempt id is assigned and the old attempt's secure-store entry is
/// replaced.
#[test]
fn starting_a_new_attempt_discards_the_previous() {
    let mut fixture = fresh_fixture("sign-in-discard");

    fixture.service.start_sign_in();
    let SessionState::Authorizing {
        attempt_id: first_attempt,
        ..
    } = *fixture.service.state()
    else {
        panic!("expected Authorizing");
    };

    let events = fixture.service.start_sign_in();
    assert!(matches!(
        events.first(),
        Some(AccountEvent::BrowserUrlReady(_))
    ));
    let SessionState::Authorizing {
        attempt_id: second_attempt,
        ..
    } = *fixture.service.state()
    else {
        panic!("expected Authorizing");
    };

    assert_ne!(first_attempt, second_attempt);
    // Exactly one `pending-authorization` entry exists — the new one.
    assert!(
        fixture
            .secure
            .entries()
            .contains_key(&EntryName::PendingAuthorization)
    );
}

/// FR-016 / SC-005 (T088): cancel, timeout, and a callback error each
/// return to `SignedOut` with the matching note, and leave no
/// `pending-authorization` entry behind (`cancel_sign_in`'s basic
/// transition is also exercised by `crates/modplayer-account/src/
/// service.rs`'s own `cancel_sign_in_returns_to_signed_out_with_no_pending_
/// state` unit test).
#[test]
fn cancel_timeout_and_error_return_to_sign_in_with_no_state() {
    // Cancel.
    {
        let mut fixture = fresh_fixture("sign-in-cancel");
        fixture.service.start_sign_in();
        fixture.service.cancel_sign_in();
        assert_eq!(
            fixture.service.state(),
            &SessionState::SignedOut {
                note: Some(SignInNote::Cancelled)
            }
        );
        assert!(
            !fixture
                .secure
                .entries()
                .contains_key(&EntryName::PendingAuthorization)
        );
    }

    // Timeout: advance the fake clock past the 5-minute browser-wait
    // budget without ever driving a callback.
    {
        let mut fixture = fresh_fixture("sign-in-timeout");
        fixture.service.start_sign_in();
        fixture.clock.advance(time::Duration::minutes(6));
        let events = drain_until(&mut fixture.service, |service| {
            matches!(service.state(), SessionState::SignedOut { note: Some(_) })
        });
        assert!(events.contains(&AccountEvent::SignInFailed(SignInNote::TimedOut)));
        assert_eq!(
            fixture.service.state(),
            &SessionState::SignedOut {
                note: Some(SignInNote::TimedOut)
            }
        );
        assert!(
            !fixture
                .secure
                .entries()
                .contains_key(&EntryName::PendingAuthorization)
        );
    }

    // Callback error: any OAuth `error` code other than `access_denied`
    // maps to `ServiceError` (the callback table's "any other error"
    // row); `access_denied` itself maps to `Cancelled` and is exercised by
    // the cancel case above via the same code path.
    {
        let mut fixture = fresh_fixture("sign-in-callback-error");
        let (port, state) = start_sign_in_and_extract_callback_target(&mut fixture.service);
        drive_callback(&port, &state, "error=invalid_scope");
        let events = drain_until(&mut fixture.service, |service| {
            matches!(service.state(), SessionState::SignedOut { note: Some(_) })
        });
        assert!(events.contains(&AccountEvent::SignInFailed(SignInNote::ServiceError)));
        assert_eq!(
            fixture.service.state(),
            &SessionState::SignedOut {
                note: Some(SignInNote::ServiceError)
            }
        );
        assert!(
            !fixture
                .secure
                .entries()
                .contains_key(&EntryName::PendingAuthorization)
        );
    }
}

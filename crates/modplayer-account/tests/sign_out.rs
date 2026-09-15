// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! US3 sign-out integration tests (contracts/account-session.md `sign_out()`,
//! FR-015, SC-003): drive `AccountService` end to end through its public
//! API, from a signed-in state reached the same way US2's `sign_in.rs`
//! tests do, with the two registry stores this slice registers
//! (`CredentialStore`, `AccountStateStore`) wired up exactly like
//! production would.

mod common;

use std::sync::Arc;
use std::time::Duration as StdDuration;

use common::{drain_until, drive_callback, fresh_fixture, profile, token_set};
use modplayer_account::fake_auth::ScriptedCall;
use modplayer_account::{AccountEvent, CredentialStore, LoadOutcome, SessionState, Tier};
use modplayer_secure_store::{EntryName, SecureStore};

/// Register the two account-scoped stores this slice defines
/// (`registry.rs` T080, `state_store.rs` T081), mirroring how production
/// wiring would register them before `launch()`.
fn register_signout_stores(fixture: &mut common::Fixture) {
    let state_store = fixture.service.state_store().clone();
    fixture
        .service
        .register_store(Box::new(CredentialStore::new(
            fixture.secure.clone() as Arc<dyn SecureStore>
        )));
    fixture.service.register_store(Box::new(state_store));
}

/// Drive a fixture from fresh to `Active` with a Premium tier, exactly like
/// US2's `callback_with_code_stores_credential_then_checks_tier`.
fn sign_in_to_active(fixture: &mut common::Fixture) {
    fixture
        .auth
        .push_exchange_code(ScriptedCall::ok(token_set()));
    fixture
        .auth
        .push_fetch_profile(ScriptedCall::ok(profile(Tier::Premium)));

    let (port, state) = common::start_sign_in_and_extract_callback_target(&mut fixture.service);
    drive_callback(&port, &state, "code=auth-code");

    drain_until(&mut fixture.service, |service| {
        matches!(service.state(), SessionState::Active)
    });
}

/// FR-015 / SC-003 (T078): `sign_out()` clears every registered store,
/// deletes the credential and `account.toml`, reports the cleared
/// categories in registry order, and leaves the session `SignedOut`.
#[test]
fn sign_out_clears_registry_stores_and_reports_categories() {
    let mut fixture = fresh_fixture("sign-out-clears");
    register_signout_stores(&mut fixture);
    sign_in_to_active(&mut fixture);
    assert!(fixture.service.playback_permitted());

    let state_store = fixture.service.state_store().clone();

    let events = fixture.service.sign_out();

    assert_eq!(
        events,
        vec![AccountEvent::SignedOut {
            categories: vec![
                "signout-category-credential",
                "signout-category-account-details",
            ],
        }]
    );
    assert_eq!(
        fixture.service.state(),
        &SessionState::SignedOut { note: None }
    );
    assert_eq!(fixture.service.tier(), Tier::Unknown);
    assert!(fixture.service.account().is_none());
    assert!(!fixture.service.playback_permitted());

    // The credential and any pending attempt are gone from the secure
    // store...
    assert!(
        fixture
            .secure
            .get(EntryName::SessionCredential)
            .unwrap()
            .is_none()
    );
    assert!(
        fixture
            .secure
            .get(EntryName::PendingAuthorization)
            .unwrap()
            .is_none()
    );
    // ...and `account.toml` no longer exists.
    assert!(matches!(state_store.load(), LoadOutcome::Absent));
}

/// FR-015 (T079): a tier check started before `sign_out()` is called is
/// abandoned — its eventual result carries the pre-sign-out `attempt_id`,
/// which `tick()` now silently drops, never reviving the session or firing
/// a tier-result event after sign-out completed.
#[test]
fn sign_out_abandons_in_flight_work() {
    let mut fixture = fresh_fixture("sign-out-abandons");
    register_signout_stores(&mut fixture);
    sign_in_to_active(&mut fixture);

    // Start a tier re-check (Settings › Account "Re-check subscription")
    // and, without waiting for it to resolve, sign out immediately —
    // `sign_out()` bumps `attempt_id` before the stray worker thread's
    // result is ever drained, so it is dropped as stale regardless of the
    // thread scheduling race.
    fixture
        .auth
        .push_fetch_profile(ScriptedCall::ok(profile(Tier::Premium)));
    fixture.service.recheck_tier();
    assert!(fixture.service.tier_check_in_flight());

    let events = fixture.service.sign_out();
    assert!(events.contains(&AccountEvent::SignedOut {
        categories: vec![
            "signout-category-credential",
            "signout-category-account-details",
        ],
    }));
    assert!(!fixture.service.tier_check_in_flight());
    assert_eq!(
        fixture.service.state(),
        &SessionState::SignedOut { note: None }
    );

    // Drain a while longer to give the stray worker thread every chance to
    // report back; its result must never resurrect the session or emit a
    // tier-result event once sign-out has already completed.
    for _ in 0..50 {
        let events = fixture.service.tick();
        assert!(
            events.is_empty(),
            "stale tier-check result was not dropped: {events:?}"
        );
        assert_eq!(
            fixture.service.state(),
            &SessionState::SignedOut { note: None }
        );
        std::thread::sleep(StdDuration::from_millis(5));
    }
    assert!(fixture.service.account().is_none());
    assert_eq!(fixture.service.tier(), Tier::Unknown);
}

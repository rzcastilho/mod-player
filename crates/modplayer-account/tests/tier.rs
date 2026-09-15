// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! US2 tier-verification integration tests (contracts/account-session.md
//! "TierCheckOutcome", FR-011).

mod common;

use common::{
    drain_until, drive_callback, fresh_fixture, profile, start_sign_in_and_extract_callback_target,
    token_set,
};
use modplayer_account::fake_auth::ScriptedCall;
use modplayer_account::{AccountEvent, SessionState, Tier};
use modplayer_secure_store::EntryName;

/// FR-011: a non-Premium tier check result still reaches `Active` (so the
/// app can show the browse-only screen instead of getting stuck), but
/// never permits playback.
#[test]
fn tier_free_and_unknown_gate_playback() {
    for tier in [Tier::Free, Tier::Unknown] {
        let mut fixture = fresh_fixture("tier-gate");
        fixture
            .auth
            .push_exchange_code(ScriptedCall::ok(token_set()));
        fixture
            .auth
            .push_fetch_profile(ScriptedCall::ok(profile(tier)));

        let (port, state) = start_sign_in_and_extract_callback_target(&mut fixture.service);
        drive_callback(&port, &state, "code=auth-code");

        drain_until(&mut fixture.service, |service| {
            matches!(service.state(), SessionState::Active)
        });

        assert_eq!(fixture.service.tier(), tier);
        assert!(!fixture.service.playback_permitted());
    }
}

/// FR-008: the tier check has a 30 s budget; whatever error comes back
/// once it expires (transport failure, timeout — indistinguishable from
/// this service's point of view, both surface as `Err` from
/// `fetch_profile`) keeps the just-written credential and settles on
/// `Active`/`Tier::Unknown` rather than getting stuck in `Checking`.
#[test]
fn tier_check_times_out_after_30s_keeping_credential() {
    let mut fixture = fresh_fixture("tier-timeout");
    fixture
        .auth
        .push_exchange_code(ScriptedCall::ok(token_set()));
    // No `push_fetch_profile` scripted: the fake's documented default for
    // an unscripted call (`AuthError::Transient`) stands in for "the 30 s
    // budget expired" — both are just an `Err` from `fetch_profile`.

    let (port, state) = start_sign_in_and_extract_callback_target(&mut fixture.service);
    drive_callback(&port, &state, "code=auth-code");

    let events = drain_until(&mut fixture.service, |service| {
        matches!(service.state(), SessionState::Active)
    });

    assert!(events.contains(&AccountEvent::TierCheckFailed));
    assert_eq!(fixture.service.tier(), Tier::Unknown);
    assert!(
        fixture
            .secure
            .entries()
            .contains_key(&EntryName::SessionCredential),
        "a failed tier check must never discard the credential"
    );
}

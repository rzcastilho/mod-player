// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! US1 "Play from account" read-integration tests (contracts/
//! account-read-delta.md, FR-022, T056). `parse_track_items`/
//! `parse_playback_state_response`'s own fixture-shaped parsing tests live
//! next to the parser in `src/spotify.rs`'s `#[cfg(test)]` module (they
//! reach a private function this crate boundary can't call); this file
//! exercises `AccountService`'s side of the contract: the fallback rule,
//! the `Forbidden` mapping, and stale-attempt dropping.

mod common;

use common::{
    drain_until, drive_callback, fresh_fixture, profile, start_sign_in_and_extract_callback_target,
    token_set,
};

use modplayer_account::fake_auth::ScriptedCall;
use modplayer_account::{AccountEvent, AuthError, ReadOutcome, SessionState, Tier};
use modplayer_audio_source::{Availability, TrackId, TrackRef};

fn track(id: &str) -> TrackRef {
    TrackRef::new(
        TrackId::new(id.to_string()).expect("valid id"),
        "Title",
        vec!["Artist".to_string()],
        None,
        None,
        1000,
        Availability::Available,
    )
}

/// Bring a fixture to `Active`/`Premium` (a readable credential is the
/// precondition for every read below), mirroring `tier.rs`'s happy path.
fn active_fixture(label: &str) -> common::Fixture {
    let mut fixture = fresh_fixture(label);
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
}

/// contracts/account-read-delta.md §1/§3: "on empty result falls back to
/// `fetch_saved_tracks`".
#[test]
fn recent_tracks_falls_back_to_saved_tracks_when_empty() {
    let mut fixture = active_fixture("reads-fallback");
    fixture.auth.push_recently_played(ScriptedCall::ok(vec![]));
    fixture
        .auth
        .push_saved_tracks(ScriptedCall::ok(vec![track("spotify:track:saved")]));

    let request_id = fixture.service.request_recent_tracks(20);

    let events = drain_until(&mut fixture.service, |_| {
        fixture.auth.call_log().len() >= 4 // authorization_url, exchange_code, fetch_profile, fetch_recently_played
    });
    // The fallback call may land on a later tick; drain a bit more.
    let mut events = events;
    events.extend(drain_until(&mut fixture.service, |_| {
        fixture
            .auth
            .call_log()
            .iter()
            .filter(|call| **call == modplayer_account::fake_auth::LoggedCall::FetchSavedTracks)
            .count()
            >= 1
    }));

    let read = events.into_iter().find_map(|event| match event {
        AccountEvent::ReadResult {
            request_id: id,
            result,
        } if id == request_id => Some(result),
        _ => None,
    });
    match read {
        Some(ReadOutcome::RecentTracks(Ok(tracks))) => {
            assert_eq!(tracks.len(), 1);
            assert_eq!(tracks[0].id.as_str(), "spotify:track:saved");
        }
        other => panic!("expected a successful fallback result, got {other:?}"),
    }
}

/// contracts/account-read-delta.md §1: "403 → `Forbidden`" — surfaced to
/// the UI as-is (the "Sign in again" inline state), never as an empty list
/// that would trigger the saved-tracks fallback.
#[test]
fn recent_tracks_forbidden_is_not_treated_as_empty() {
    let mut fixture = active_fixture("reads-forbidden");
    fixture
        .auth
        .push_recently_played(ScriptedCall::err(AuthError::Forbidden));

    let request_id = fixture.service.request_recent_tracks(20);
    let events = drain_until(&mut fixture.service, |_| {
        fixture
            .auth
            .call_log()
            .contains(&modplayer_account::fake_auth::LoggedCall::FetchRecentlyPlayed)
    });

    let read = events.into_iter().find_map(|event| match event {
        AccountEvent::ReadResult {
            request_id: id,
            result,
        } if id == request_id => Some(result),
        _ => None,
    });
    assert_eq!(
        read,
        Some(ReadOutcome::RecentTracks(Err(AuthError::Forbidden)))
    );
    assert!(
        !fixture
            .auth
            .call_log()
            .contains(&modplayer_account::fake_auth::LoggedCall::FetchSavedTracks),
        "a Forbidden result must not trigger the saved-tracks fallback"
    );
}

/// contracts/account-read-delta.md §1: `204` → `Ok(None)`.
#[test]
fn playback_state_204_maps_to_none() {
    let mut fixture = active_fixture("reads-playback-state");
    fixture.auth.push_playback_state(ScriptedCall::ok(None));

    let request_id = fixture.service.request_playback_state();
    let events = drain_until(&mut fixture.service, |_| {
        fixture
            .auth
            .call_log()
            .contains(&modplayer_account::fake_auth::LoggedCall::FetchPlaybackState)
    });

    let read = events.into_iter().find_map(|event| match event {
        AccountEvent::ReadResult {
            request_id: id,
            result,
        } if id == request_id => Some(result),
        _ => None,
    });
    assert_eq!(read, Some(ReadOutcome::PlaybackState(Ok(None))));
}

/// contracts/account-read-delta.md §3: "a result for a stale attempt
/// (sign-out in between) is dropped" — the request is issued, then
/// `sign_out()` bumps the session generation before the worker's result is
/// drained; `tick()` must never raise a `ReadResult` for it.
#[test]
fn stale_read_after_sign_out_is_dropped() {
    let mut fixture = active_fixture("reads-stale");
    fixture
        .auth
        .push_recently_played(ScriptedCall::ok(vec![track("spotify:track:x")]));

    let _request_id = fixture.service.request_recent_tracks(20);
    fixture.service.sign_out();

    let mut saw_read_result = false;
    for _ in 0..50 {
        for event in fixture.service.tick() {
            if matches!(event, AccountEvent::ReadResult { .. }) {
                saw_read_result = true;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(
        !saw_read_result,
        "a read started before sign-out must never surface after it"
    );
}

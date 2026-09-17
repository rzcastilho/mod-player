// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! `SyncScheduler` (data-model.md §3.3, contracts/library-and-search-
//! core.md §1/§4): launch/interval/reconnect triggers, page walking,
//! rate-limit backoff, silent failure vs. FR-021 first-sync-failed
//! (derived from whether `LibraryIndex` already holds a snapshot), resume
//! after offline, and a `Partial` outcome on an `Unsupported` set.

use std::time::{Duration, Instant};

use modplayer_audio_source::{
    Availability, CatalogError, LibraryItem, LibraryPage, LibrarySet, SourceCommand, TrackId,
    TrackRef,
};
use modplayer_core::library::index::{LibraryIndex, SyncOutcome};
use modplayer_core::library::sync::{SYNC_INTERVAL, SchedulerState, SyncScheduler};

fn empty_page(set: LibrarySet) -> LibraryPage {
    LibraryPage {
        set,
        items: vec![],
        next_page: None,
        sync_token: None,
    }
}

fn track_page(set: LibrarySet, id: &str) -> LibraryPage {
    LibraryPage {
        set,
        items: vec![LibraryItem::Track {
            track: TrackRef::new(
                TrackId::new(id).unwrap(),
                "Song",
                vec![],
                None,
                None,
                1000,
                Availability::Available,
            ),
            added_at: None,
        }],
        next_page: None,
        sync_token: None,
    }
}

/// Walk a full cycle, feeding `result_for` a fresh outcome per set;
/// returns the scheduler's recorded outcome for the completed cycle.
fn run_cycle(
    scheduler: &mut SyncScheduler,
    index: &mut LibraryIndex,
    now: Instant,
    mut result_for: impl FnMut(LibrarySet) -> Result<LibraryPage, CatalogError>,
) -> Option<SyncOutcome> {
    let mut commands = scheduler.trigger(now);
    while let Some(SourceCommand::FetchLibrary {
        request_id, set, ..
    }) = commands.first().cloned()
    {
        let outcome = scheduler.apply_reply(now, request_id, result_for(set));
        if let Some(page) = outcome.merged_page {
            index.merge_page(page);
        }
        commands = outcome.commands;
    }
    scheduler.take_last_outcome()
}

#[test]
fn launch_trigger_starts_the_first_set_of_the_cycle() {
    let mut scheduler = SyncScheduler::new();
    let commands = scheduler.trigger(Instant::now());
    assert_eq!(commands.len(), 1);
    assert!(matches!(scheduler.state(), SchedulerState::Syncing { .. }));
}

#[test]
fn a_full_cycle_of_ok_pages_walks_every_set_and_reports_ok() {
    let mut scheduler = SyncScheduler::new();
    let mut index = LibraryIndex::new();
    let outcome = run_cycle(&mut scheduler, &mut index, Instant::now(), |set| {
        Ok(empty_page(set))
    });
    assert_eq!(outcome, Some(SyncOutcome::Ok));
    assert_eq!(scheduler.state(), &SchedulerState::Idle);
}

#[test]
fn pages_merge_into_the_index_as_the_cycle_walks() {
    let mut scheduler = SyncScheduler::new();
    let mut index = LibraryIndex::new();
    run_cycle(&mut scheduler, &mut index, Instant::now(), |set| {
        if set == LibrarySet::SavedTracks {
            Ok(track_page(set, "spotify:track:a"))
        } else {
            Ok(empty_page(set))
        }
    });
    assert_eq!(index.saved_tracks().len(), 1);
}

#[test]
fn interval_trigger_is_silent_before_15_minutes_and_fires_after() {
    let mut scheduler = SyncScheduler::new();
    let mut index = LibraryIndex::new();
    let now = Instant::now();
    run_cycle(&mut scheduler, &mut index, now, |set| Ok(empty_page(set)));

    assert!(
        scheduler
            .tick(now + Duration::from_secs(30), true)
            .is_empty()
    );
    let commands = scheduler.tick(now + SYNC_INTERVAL + Duration::from_secs(1), true);
    assert!(!commands.is_empty());
}

#[test]
fn reconnect_trigger_restarts_the_cycle_even_before_the_interval_elapses() {
    let mut scheduler = SyncScheduler::new();
    let mut index = LibraryIndex::new();
    let now = Instant::now();
    run_cycle(&mut scheduler, &mut index, now, |set| Ok(empty_page(set)));

    // Simulate going offline then reconnecting well within the interval.
    scheduler.tick(now, false);
    let commands = scheduler.trigger(now + Duration::from_secs(5));
    assert!(!commands.is_empty(), "Reconnect must trigger a fresh cycle");
}

#[test]
fn rate_limit_backs_off_and_resumes_the_same_set_after_the_deadline() {
    let mut scheduler = SyncScheduler::new();
    let now = Instant::now();
    scheduler.trigger(now);
    scheduler.apply_reply(
        now,
        0,
        Err(CatalogError::RateLimited {
            retry_after_ms: Some(500),
        }),
    );
    assert!(scheduler.refreshing());
    assert!(scheduler.tick(now, true).is_empty());
    let commands = scheduler.tick(now + Duration::from_secs(1), true);
    assert!(matches!(
        commands.as_slice(),
        [SourceCommand::FetchLibrary {
            set: LibrarySet::SavedTracks,
            ..
        }]
    ));
}

#[test]
fn a_silent_failure_with_a_prior_snapshot_never_reports_first_sync_failed() {
    let mut scheduler = SyncScheduler::new();
    let mut index = LibraryIndex::new();
    let now = Instant::now();

    // A prior successful cycle leaves a snapshot behind.
    run_cycle(&mut scheduler, &mut index, now, |set| {
        if set == LibrarySet::SavedTracks {
            Ok(track_page(set, "spotify:track:a"))
        } else {
            Ok(empty_page(set))
        }
    });
    assert!(index.has_any_snapshot());

    // A later cycle fails outright — silent, since a snapshot exists
    // (data-model.md §3.3 "other error, snapshot exists -> silent Failed").
    let outcome = run_cycle(&mut scheduler, &mut index, now, |_| {
        Err(CatalogError::Offline)
    });
    assert_eq!(outcome, Some(SyncOutcome::Failed));
    let first_sync_failed = outcome == Some(SyncOutcome::Failed) && !index.has_any_snapshot();
    assert!(
        !first_sync_failed,
        "a snapshot exists — this must stay silent"
    );
}

#[test]
fn a_failure_with_no_snapshot_is_the_fr021_first_sync_failed_state() {
    let mut scheduler = SyncScheduler::new();
    let index = LibraryIndex::new();
    let now = Instant::now();
    let outcome = {
        let mut commands = scheduler.trigger(now);
        let mut last = None;
        while let Some(SourceCommand::FetchLibrary { request_id, .. }) = commands.first().cloned() {
            let reply = scheduler.apply_reply(now, request_id, Err(CatalogError::Offline));
            last = scheduler.take_last_outcome().or(last);
            commands = reply.commands;
        }
        last
    };
    assert_eq!(outcome, Some(SyncOutcome::Failed));
    assert!(!index.has_any_snapshot());
    let first_sync_failed = outcome == Some(SyncOutcome::Failed) && !index.has_any_snapshot();
    assert!(first_sync_failed);
}

#[test]
fn going_offline_mid_cycle_resets_to_idle_and_a_later_trigger_still_works() {
    let mut scheduler = SyncScheduler::new();
    let mut index = LibraryIndex::new();
    let now = Instant::now();
    // A completed cycle first, so the passive `Interval` check (which
    // treats "never synced" as always due) doesn't mask what this test
    // means to exercise.
    run_cycle(&mut scheduler, &mut index, now, |set| Ok(empty_page(set)));

    scheduler.trigger(now);
    scheduler.tick(now, false);
    assert_eq!(scheduler.state(), &SchedulerState::Idle);
    // Not due for another automatic interval yet, but nothing is stuck
    // either — an explicit trigger (Reconnect) is what actually restarts
    // it, matching data-model.md §3.3 exactly.
    assert!(scheduler.tick(now, true).is_empty());
    assert!(!scheduler.trigger(now).is_empty());
}

#[test]
fn a_never_synced_scheduler_treats_the_interval_check_as_immediately_due() {
    // Self-healing: if a `Launch` trigger were ever missed, the passive
    // per-tick check still gets a first sync going rather than waiting a
    // full 15 minutes for a library that has never synced at all.
    let mut scheduler = SyncScheduler::new();
    let commands = scheduler.tick(Instant::now(), true);
    assert!(!commands.is_empty());
}

#[test]
fn an_unsupported_set_yields_a_partial_outcome_and_the_cycle_still_completes() {
    let mut scheduler = SyncScheduler::new();
    let mut index = LibraryIndex::new();
    let outcome = run_cycle(&mut scheduler, &mut index, Instant::now(), |set| {
        if set == LibrarySet::FollowedArtists {
            Err(CatalogError::Unsupported)
        } else {
            Ok(empty_page(set))
        }
    });
    assert_eq!(outcome, Some(SyncOutcome::Partial));
}

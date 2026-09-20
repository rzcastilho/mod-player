// SPDX-License-Identifier: MIT OR Apache-2.0

//! `SyncScheduler`: a pure, injectable-clock state machine driving one
//! `FetchLibrary` cycle across the four library sets (data-model.md §3.3,
//! research R6). No I/O, no egui — `tick`/`trigger` return the
//! `SourceCommand`s to issue and `apply_reply` folds a
//! `SourceEvent::LibraryPage` reply back in, exactly like `SearchSession`
//! (research R7).

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use modplayer_audio_source::{CatalogError, LibraryPage, LibrarySet, SourceCommand};

use super::index::SyncOutcome;

/// contracts/library-and-search-core.md §4.
pub const SYNC_INTERVAL: Duration = Duration::from_secs(15 * 60);
/// contracts/library-and-search-core.md §4: `15 s x 2^attempt, max 4 min`.
const SYNC_BACKOFF_BASE: Duration = Duration::from_secs(15);
const SYNC_BACKOFF_MAX: Duration = Duration::from_secs(4 * 60);
/// contracts/library-and-search-core.md §4.
pub const SYNC_PAGE: u16 = 200;

/// The fixed cycle order (data-model.md §3.1 lists all four; rootlist-
/// backed `Playlists` last since it fully resolves each item itself and so
/// benefits least from running before the others).
const SET_ORDER: [LibrarySet; 4] = [
    LibrarySet::SavedTracks,
    LibrarySet::SavedAlbums,
    LibrarySet::FollowedArtists,
    LibrarySet::Playlists,
];

/// data-model.md §3.3.
#[derive(Debug, Clone, PartialEq)]
pub enum SchedulerState {
    Idle,
    Syncing {
        set: LibrarySet,
        page: Option<String>,
        started: Instant,
    },
    BackingOff {
        until: Instant,
        attempt: u8,
    },
}

/// What one `apply_reply` call produced: the page to merge into
/// `LibraryIndex` (if any) and the follow-up commands to issue.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SyncReplyOutcome {
    pub merged_page: Option<LibraryPage>,
    pub commands: Vec<SourceCommand>,
}

/// A `FetchLibrary` cycle across the four sets (data-model.md §3.3).
#[derive(Debug, Clone, PartialEq)]
pub struct SyncScheduler {
    state: SchedulerState,
    current_set: Option<LibrarySet>,
    current_page: Option<String>,
    queued_sets: VecDeque<LibrarySet>,
    cycle_partial: bool,
    cycle_failed: bool,
    backoff_attempt: u8,
    /// Wall time of the last *completed* cycle — gates the 15 min
    /// `Interval` trigger. Injected-clock `Instant`, not persisted (the
    /// persisted `last_synced_at` unix-ms lives in `LibraryIndex::meta`,
    /// set by the controller from [`Self::take_last_outcome`]).
    last_synced_at: Option<Instant>,
    next_request_id: u64,
    in_flight_request_id: Option<u64>,
    last_cycle_outcome: Option<SyncOutcome>,
}

impl Default for SyncScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl SyncScheduler {
    pub fn new() -> Self {
        Self {
            state: SchedulerState::Idle,
            current_set: None,
            current_page: None,
            queued_sets: VecDeque::new(),
            cycle_partial: false,
            cycle_failed: false,
            backoff_attempt: 0,
            last_synced_at: None,
            next_request_id: 0,
            in_flight_request_id: None,
            last_cycle_outcome: None,
        }
    }

    pub fn state(&self) -> &SchedulerState {
        &self.state
    }

    /// FR-015/§3.3 "Derived view flags": `true` only while backed off after
    /// a rate limit.
    pub fn refreshing(&self) -> bool {
        matches!(self.state, SchedulerState::BackingOff { .. })
    }

    /// Whether a cycle is currently running (`Syncing` or `BackingOff`).
    pub fn is_syncing(&self) -> bool {
        !matches!(self.state, SchedulerState::Idle)
    }

    /// Drain the outcome the most recently *completed* cycle recorded, if
    /// any — the controller persists it into `LibraryIndex.meta` once.
    pub fn take_last_outcome(&mut self) -> Option<SyncOutcome> {
        self.last_cycle_outcome.take()
    }

    /// `Launch`/`Reconnect`/`Retry` triggers (data-model.md §3.3):
    /// (re)start a fresh cycle now, abandoning whatever the scheduler was
    /// doing.
    ///
    /// ```
    /// use std::time::Instant;
    ///
    /// use modplayer_audio_source::SourceCommand;
    /// use modplayer_core::library::SyncScheduler;
    ///
    /// let mut scheduler = SyncScheduler::new();
    /// let commands = scheduler.trigger(Instant::now());
    /// assert!(matches!(
    ///     commands.as_slice(),
    ///     [SourceCommand::FetchLibrary { .. }]
    /// ));
    /// assert!(scheduler.is_syncing());
    /// ```
    pub fn trigger(&mut self, now: Instant) -> Vec<SourceCommand> {
        self.start_cycle(now)
    }

    /// `Interval`/`BackingOff`-resume trigger, polled every `tick()`
    /// (data-model.md §3.3): 15 min since the last completed cycle while
    /// `Idle`, or the backoff deadline elapsing while `BackingOff`. Going
    /// offline resets to `Idle` and discards the in-flight request
    /// (data-model.md §3.3: "in-flight replies discarded; resumes on
    /// Reconnect").
    pub fn tick(&mut self, now: Instant, online: bool) -> Vec<SourceCommand> {
        if !online {
            if !matches!(self.state, SchedulerState::Idle) {
                self.state = SchedulerState::Idle;
                self.in_flight_request_id = None;
            }
            return Vec::new();
        }
        match self.state.clone() {
            SchedulerState::Idle => {
                let due = self
                    .last_synced_at
                    .is_none_or(|since| now.duration_since(since) >= SYNC_INTERVAL);
                if due {
                    self.start_cycle(now)
                } else {
                    Vec::new()
                }
            }
            SchedulerState::BackingOff { until, .. } => {
                if now >= until {
                    self.issue_current(now)
                } else {
                    Vec::new()
                }
            }
            SchedulerState::Syncing { .. } => Vec::new(),
        }
    }

    fn start_cycle(&mut self, now: Instant) -> Vec<SourceCommand> {
        let mut sets: VecDeque<LibrarySet> = SET_ORDER.into_iter().collect();
        self.current_set = sets.pop_front();
        self.queued_sets = sets;
        self.current_page = None;
        self.cycle_partial = false;
        self.cycle_failed = false;
        self.backoff_attempt = 0;
        self.issue_current(now)
    }

    fn issue_current(&mut self, now: Instant) -> Vec<SourceCommand> {
        let Some(set) = self.current_set else {
            self.complete_cycle(now);
            return Vec::new();
        };
        let request_id = self.next_request_id;
        self.next_request_id += 1;
        self.in_flight_request_id = Some(request_id);
        self.state = SchedulerState::Syncing {
            set,
            page: self.current_page.clone(),
            started: now,
        };
        vec![SourceCommand::FetchLibrary {
            request_id,
            set,
            page: self.current_page.clone(),
            limit: SYNC_PAGE,
        }]
    }

    fn complete_cycle(&mut self, now: Instant) {
        self.state = SchedulerState::Idle;
        self.last_synced_at = Some(now);
        self.last_cycle_outcome = Some(if self.cycle_failed {
            SyncOutcome::Failed
        } else if self.cycle_partial {
            SyncOutcome::Partial
        } else {
            SyncOutcome::Ok
        });
    }

    fn advance_to_next_set(&mut self, now: Instant) -> Vec<SourceCommand> {
        self.current_page = None;
        self.current_set = self.queued_sets.pop_front();
        self.issue_current(now)
    }

    /// Fold a `SourceEvent::LibraryPage` reply matching `request_id`
    /// (contracts/library-and-search-core.md §1); a stale/mismatched id is
    /// ignored (empty outcome).
    pub fn apply_reply(
        &mut self,
        now: Instant,
        request_id: u64,
        result: Result<LibraryPage, CatalogError>,
    ) -> SyncReplyOutcome {
        if self.in_flight_request_id != Some(request_id) {
            return SyncReplyOutcome::default();
        }
        self.in_flight_request_id = None;
        match result {
            Ok(page) => {
                self.current_page = page.next_page.clone();
                let has_more = page.next_page.is_some();
                let commands = if has_more {
                    self.issue_current(now)
                } else {
                    self.advance_to_next_set(now)
                };
                SyncReplyOutcome {
                    merged_page: Some(page),
                    commands,
                }
            }
            Err(CatalogError::RateLimited { retry_after_ms }) => {
                let delay = backoff_delay(self.backoff_attempt, retry_after_ms);
                self.state = SchedulerState::BackingOff {
                    until: now + delay,
                    attempt: self.backoff_attempt,
                };
                self.backoff_attempt = self.backoff_attempt.saturating_add(1);
                SyncReplyOutcome::default()
            }
            Err(CatalogError::Unsupported) => {
                self.cycle_partial = true;
                SyncReplyOutcome {
                    merged_page: None,
                    commands: self.advance_to_next_set(now),
                }
            }
            Err(_other) => {
                // "other error, snapshot exists -> silent Failed; no
                // snapshot -> FirstSyncFailed" (data-model.md §3.3): the
                // scheduler doesn't know whether a snapshot exists — it
                // just records the cycle's own outcome; the controller
                // decides silent vs. FR-021 from `LibraryIndex::
                // has_any_snapshot`.
                self.cycle_failed = true;
                SyncReplyOutcome {
                    merged_page: None,
                    commands: self.advance_to_next_set(now),
                }
            }
        }
    }
}

fn backoff_delay(attempt: u8, retry_after_ms: Option<u32>) -> Duration {
    if let Some(ms) = retry_after_ms {
        return Duration::from_millis(u64::from(ms)).min(SYNC_BACKOFF_MAX);
    }
    let factor = 1u64
        .checked_shl(u32::from(attempt.min(10)))
        .unwrap_or(1 << 10);
    (SYNC_BACKOFF_BASE * factor as u32).min(SYNC_BACKOFF_MAX)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;

    fn empty_page(set: LibrarySet) -> LibraryPage {
        LibraryPage {
            set,
            items: vec![],
            next_page: None,
            sync_token: None,
        }
    }

    #[test]
    fn launch_trigger_issues_a_fetch_for_the_first_set() {
        let mut scheduler = SyncScheduler::new();
        let now = Instant::now();
        let commands = scheduler.trigger(now);
        assert!(matches!(
            commands.as_slice(),
            [SourceCommand::FetchLibrary {
                set: LibrarySet::SavedTracks,
                page: None,
                ..
            }]
        ));
    }

    #[test]
    fn a_successful_reply_with_no_next_page_advances_to_the_next_set() {
        let mut scheduler = SyncScheduler::new();
        let now = Instant::now();
        scheduler.trigger(now);
        let outcome = scheduler.apply_reply(now, 0, Ok(empty_page(LibrarySet::SavedTracks)));
        assert!(matches!(
            outcome.commands.as_slice(),
            [SourceCommand::FetchLibrary {
                set: LibrarySet::SavedAlbums,
                ..
            }]
        ));
    }

    #[test]
    fn a_next_page_token_re_requests_the_same_set() {
        let mut scheduler = SyncScheduler::new();
        let now = Instant::now();
        scheduler.trigger(now);
        let mut page = empty_page(LibrarySet::SavedTracks);
        page.next_page = Some("cursor-1".to_string());
        let outcome = scheduler.apply_reply(now, 0, Ok(page));
        assert!(matches!(
            outcome.commands.as_slice(),
            [SourceCommand::FetchLibrary {
                set: LibrarySet::SavedTracks,
                page: Some(token),
                ..
            }] if token == "cursor-1"
        ));
    }

    #[test]
    fn walking_all_four_sets_completes_the_cycle_with_ok_outcome() {
        let mut scheduler = SyncScheduler::new();
        let now = Instant::now();
        let mut commands = scheduler.trigger(now);
        for i in 0..4u64 {
            let SourceCommand::FetchLibrary { set, .. } = commands[0].clone() else {
                unreachable!()
            };
            let outcome = scheduler.apply_reply(now, i, Ok(empty_page(set)));
            commands = outcome.commands;
        }
        assert!(commands.is_empty());
        assert_eq!(scheduler.state(), &SchedulerState::Idle);
        assert_eq!(scheduler.take_last_outcome(), Some(SyncOutcome::Ok));
    }

    #[test]
    fn a_rate_limited_reply_backs_off_and_keeps_stale_data_flagged_refreshing() {
        let mut scheduler = SyncScheduler::new();
        let now = Instant::now();
        scheduler.trigger(now);
        let outcome = scheduler.apply_reply(
            now,
            0,
            Err(CatalogError::RateLimited {
                retry_after_ms: None,
            }),
        );
        assert!(outcome.commands.is_empty());
        assert!(scheduler.refreshing());
    }

    #[test]
    fn backoff_resumes_the_same_set_and_page_once_the_deadline_passes() {
        let mut scheduler = SyncScheduler::new();
        let now = Instant::now();
        scheduler.trigger(now);
        scheduler.apply_reply(
            now,
            0,
            Err(CatalogError::RateLimited {
                retry_after_ms: Some(1_000),
            }),
        );
        let commands = scheduler.tick(now, true);
        assert!(commands.is_empty(), "backoff not yet elapsed");
        let later = now + Duration::from_secs(2);
        let commands = scheduler.tick(later, true);
        assert!(matches!(
            commands.as_slice(),
            [SourceCommand::FetchLibrary {
                set: LibrarySet::SavedTracks,
                ..
            }]
        ));
        assert!(!scheduler.refreshing());
    }

    #[test]
    fn an_unsupported_set_marks_the_cycle_partial_and_moves_on() {
        let mut scheduler = SyncScheduler::new();
        let now = Instant::now();
        let mut commands = scheduler.trigger(now);
        for i in 0..4u64 {
            let SourceCommand::FetchLibrary { set, .. } = commands[0].clone() else {
                unreachable!()
            };
            let result = if set == LibrarySet::SavedAlbums {
                Err(CatalogError::Unsupported)
            } else {
                Ok(empty_page(set))
            };
            let outcome = scheduler.apply_reply(now, i, result);
            commands = outcome.commands;
        }
        assert_eq!(scheduler.take_last_outcome(), Some(SyncOutcome::Partial));
    }

    #[test]
    fn a_silent_failure_marks_the_cycle_failed_and_still_walks_the_rest() {
        let mut scheduler = SyncScheduler::new();
        let now = Instant::now();
        let mut commands = scheduler.trigger(now);
        for i in 0..4u64 {
            let SourceCommand::FetchLibrary { set, .. } = commands[0].clone() else {
                unreachable!()
            };
            let result = if set == LibrarySet::SavedTracks {
                Err(CatalogError::Offline)
            } else {
                Ok(empty_page(set))
            };
            let outcome = scheduler.apply_reply(now, i, result);
            commands = outcome.commands;
        }
        assert_eq!(scheduler.take_last_outcome(), Some(SyncOutcome::Failed));
    }

    #[test]
    fn interval_trigger_fires_15_minutes_after_the_last_completed_cycle() {
        let mut scheduler = SyncScheduler::new();
        let now = Instant::now();
        let mut commands = scheduler.trigger(now);
        for i in 0..4u64 {
            let SourceCommand::FetchLibrary { set, .. } = commands[0].clone() else {
                unreachable!()
            };
            commands = scheduler.apply_reply(now, i, Ok(empty_page(set))).commands;
        }
        assert!(
            scheduler
                .tick(now + Duration::from_secs(60), true)
                .is_empty()
        );
        let commands = scheduler.tick(now + SYNC_INTERVAL + Duration::from_secs(1), true);
        assert!(!commands.is_empty());
    }

    #[test]
    fn going_offline_mid_cycle_resets_to_idle_and_discards_the_in_flight_request() {
        let mut scheduler = SyncScheduler::new();
        let now = Instant::now();
        scheduler.trigger(now);
        scheduler.tick(now, false);
        assert_eq!(scheduler.state(), &SchedulerState::Idle);
        // A reply for the discarded request no longer matches anything.
        let outcome = scheduler.apply_reply(now, 0, Ok(empty_page(LibrarySet::SavedTracks)));
        assert_eq!(outcome, SyncReplyOutcome::default());
    }

    #[test]
    fn reconnect_trigger_restarts_a_fresh_cycle_immediately() {
        let mut scheduler = SyncScheduler::new();
        let now = Instant::now();
        scheduler.trigger(now);
        scheduler.tick(now, false);
        let commands = scheduler.trigger(now + Duration::from_secs(1));
        assert!(!commands.is_empty());
    }
}

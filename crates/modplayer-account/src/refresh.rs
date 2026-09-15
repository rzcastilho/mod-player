// SPDX-License-Identifier: MIT OR Apache-2.0

//! `RefreshScheduler` (data-model.md §2.3, contracts/account-session.md
//! "Refresh scheduler rules", FR-013, FR-014, FR-021): when the next
//! refresh is due, exponential backoff on transient failure, and the
//! `Expired`/`SessionExpired` rule when no refresh succeeds before
//! `expires_at`. Pure decision logic driven by an injected [`Clock`]
//! (Constitution VIII) — no `sleep`, no I/O; `AccountService` is the only
//! caller that actually performs a refresh.

use time::{Duration, OffsetDateTime};

/// Refresh is armed at `expires_at - 5 min`, or at `expires_at -
/// lifetime/2` when the credential's total lifetime is under 10 minutes
/// (FR-013).
const NORMAL_LEAD: Duration = Duration::minutes(5);
const SHORT_LIFETIME_THRESHOLD: Duration = Duration::minutes(10);

/// Backoff starts at 1 s and doubles up to a 60 s cap (FR-014).
const BACKOFF_INITIAL: Duration = Duration::seconds(1);
const BACKOFF_CAP: Duration = Duration::seconds(60);

/// Consecutive transient failures at which `RefreshFailing` is raised
/// (contracts/account-session.md).
const FAILING_THRESHOLD: u32 = 3;

/// What the scheduler wants `AccountService::tick()` to do this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshAction {
    /// Nothing due yet.
    Wait,
    /// Spawn an `account-refresh` worker now.
    RefreshNow,
}

/// The result of reporting an outcome back to the scheduler
/// (contracts/account-session.md "Refresh scheduler rules").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshReport {
    /// Nothing notable — a transient failure that has not yet reached the
    /// `RefreshFailing` threshold, or a success while no failures had been
    /// reported.
    None,
    /// The 3rd consecutive transient failure: raise `RefreshFailing` once.
    Failing,
    /// A success after one or more `RefreshFailing`-triggering failures:
    /// dismiss the notification.
    Recovered,
}

/// Per-`Active` (or `Expired`) session refresh timer and backoff state
/// (data-model.md §2.3). Owns no credential and performs no I/O itself —
/// `AccountService` asks [`RefreshScheduler::action`] each tick, and
/// reports the outcome of any refresh it actually performed via
/// [`RefreshScheduler::on_success`]/[`RefreshScheduler::on_transient_failure`].
#[derive(Debug, Clone, Copy)]
pub struct RefreshScheduler {
    due_at: OffsetDateTime,
    consecutive_failures: u32,
    /// Set once `RefreshFailing` has been raised, so a later success knows
    /// to report `Recovered` (and so `Failing` is only ever reported once
    /// per losing streak).
    failing_raised: bool,
    /// Whether a refresh is currently in flight — at most one at a time
    /// (contracts/account-session.md).
    in_flight: bool,
}

impl RefreshScheduler {
    /// Arm a fresh scheduler for a credential expiring at `expires_at`,
    /// authorized at `authorized_at` (used only to derive the credential's
    /// total lifetime for the short-lifetime rule, FR-013).
    pub fn new(authorized_at: OffsetDateTime, expires_at: OffsetDateTime) -> Self {
        Self {
            due_at: due_at_for(authorized_at, expires_at),
            consecutive_failures: 0,
            failing_raised: false,
            in_flight: false,
        }
    }

    /// When this scheduler is next due to refresh.
    pub fn due_at(&self) -> OffsetDateTime {
        self.due_at
    }

    /// Number of consecutive transient failures since the last success.
    pub fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures
    }

    /// Whether a refresh is currently in flight.
    pub fn is_in_flight(&self) -> bool {
        self.in_flight
    }

    /// What to do this tick (contracts/account-session.md: "At most one
    /// refresh in flight"). Marks a refresh as in-flight when it says
    /// `RefreshNow` — the caller must eventually call `on_success` or
    /// `on_transient_failure` to clear it.
    pub fn action(&mut self, now: OffsetDateTime) -> RefreshAction {
        if self.in_flight {
            return RefreshAction::Wait;
        }
        if now >= self.due_at {
            self.in_flight = true;
            RefreshAction::RefreshNow
        } else {
            RefreshAction::Wait
        }
    }

    /// A refresh succeeded: rearm the timer from the new expiry, reset the
    /// backoff counters, and report whether a `RefreshFailing` notification
    /// needs to be dismissed (contracts/account-session.md).
    pub fn on_success(
        &mut self,
        authorized_at: OffsetDateTime,
        expires_at: OffsetDateTime,
    ) -> RefreshReport {
        self.in_flight = false;
        self.due_at = due_at_for(authorized_at, expires_at);
        self.consecutive_failures = 0;
        if self.failing_raised {
            self.failing_raised = false;
            RefreshReport::Recovered
        } else {
            RefreshReport::None
        }
    }

    /// A refresh failed transiently: reschedule after the backoff delay
    /// (`min(60s, 1s * 2^n)`), increment the failure count, and report
    /// whether this is the 3rd consecutive failure (contracts/account-
    /// session.md).
    pub fn on_transient_failure(&mut self, now: OffsetDateTime) -> RefreshReport {
        self.in_flight = false;
        self.consecutive_failures += 1;
        self.due_at = now + backoff_delay(self.consecutive_failures);
        if self.consecutive_failures == FAILING_THRESHOLD && !self.failing_raised {
            self.failing_raised = true;
            RefreshReport::Failing
        } else {
            RefreshReport::None
        }
    }
}

/// `due_at = expires_at - 5 min`, or `expires_at - lifetime/2` when
/// `lifetime < 10 min` (FR-013, data-model.md §2.3).
fn due_at_for(authorized_at: OffsetDateTime, expires_at: OffsetDateTime) -> OffsetDateTime {
    let lifetime = expires_at - authorized_at;
    if lifetime < SHORT_LIFETIME_THRESHOLD {
        expires_at - lifetime / 2
    } else {
        expires_at - NORMAL_LEAD
    }
}

/// `min(60s, 1s * 2^(failures - 1))`: 1s, 2s, 4s, 8s, 16s, 32s, 60s, 60s, …
fn backoff_delay(consecutive_failures: u32) -> Duration {
    let exponent = consecutive_failures.saturating_sub(1).min(6);
    let delay = BACKOFF_INITIAL * 2i32.pow(exponent);
    delay.min(BACKOFF_CAP)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn epoch() -> OffsetDateTime {
        OffsetDateTime::UNIX_EPOCH
    }

    /// FR-013: due 5 minutes before a normal-lifetime expiry, or at the
    /// half-lifetime point when the credential's total lifetime is under
    /// 10 minutes (T053).
    #[test]
    fn refresh_is_scheduled_5min_before_expiry_or_half_lifetime() {
        // One-hour lifetime: due 5 minutes before expiry.
        let authorized_at = epoch();
        let expires_at = epoch() + Duration::hours(1);
        let scheduler = RefreshScheduler::new(authorized_at, expires_at);
        assert_eq!(scheduler.due_at(), expires_at - Duration::minutes(5));

        // 6-minute lifetime (< 10 min): due at half the lifetime, not 5
        // minutes before expiry (which would be in the past relative to
        // authorization).
        let expires_at_short = epoch() + Duration::minutes(6);
        let scheduler_short = RefreshScheduler::new(authorized_at, expires_at_short);
        assert_eq!(
            scheduler_short.due_at(),
            expires_at_short - Duration::minutes(3)
        );
    }

    #[test]
    fn action_waits_until_due_then_marks_in_flight() {
        let authorized_at = epoch();
        let expires_at = epoch() + Duration::hours(1);
        let mut scheduler = RefreshScheduler::new(authorized_at, expires_at);

        assert_eq!(scheduler.action(epoch()), RefreshAction::Wait);
        assert!(!scheduler.is_in_flight());

        let due = scheduler.due_at();
        assert_eq!(scheduler.action(due), RefreshAction::RefreshNow);
        assert!(scheduler.is_in_flight());

        // Already in flight: no second refresh spawned even though still due.
        assert_eq!(scheduler.action(due), RefreshAction::Wait);
    }

    #[test]
    fn backoff_doubles_and_caps_at_60s() {
        assert_eq!(backoff_delay(1), Duration::seconds(1));
        assert_eq!(backoff_delay(2), Duration::seconds(2));
        assert_eq!(backoff_delay(3), Duration::seconds(4));
        assert_eq!(backoff_delay(4), Duration::seconds(8));
        assert_eq!(backoff_delay(5), Duration::seconds(16));
        assert_eq!(backoff_delay(6), Duration::seconds(32));
        assert_eq!(backoff_delay(7), Duration::seconds(60));
        assert_eq!(backoff_delay(100), Duration::seconds(60));
    }

    #[test]
    fn on_transient_failure_reschedules_with_backoff_and_increments_count() {
        let mut scheduler = RefreshScheduler::new(epoch(), epoch() + Duration::hours(1));
        let due = scheduler.due_at();
        assert_eq!(scheduler.action(due), RefreshAction::RefreshNow);

        let report = scheduler.on_transient_failure(due);
        assert_eq!(report, RefreshReport::None);
        assert_eq!(scheduler.consecutive_failures(), 1);
        assert_eq!(scheduler.due_at(), due + Duration::seconds(1));
        assert!(!scheduler.is_in_flight());
    }

    /// FR-014: the 3rd consecutive transient failure raises `Failing`
    /// exactly once.
    #[test]
    fn third_consecutive_failure_reports_failing_once() {
        let mut scheduler = RefreshScheduler::new(epoch(), epoch() + Duration::hours(1));
        let mut now = scheduler.due_at();

        assert_eq!(scheduler.action(now), RefreshAction::RefreshNow);
        assert_eq!(scheduler.on_transient_failure(now), RefreshReport::None);

        now = scheduler.due_at();
        assert_eq!(scheduler.action(now), RefreshAction::RefreshNow);
        assert_eq!(scheduler.on_transient_failure(now), RefreshReport::None);

        now = scheduler.due_at();
        assert_eq!(scheduler.action(now), RefreshAction::RefreshNow);
        assert_eq!(scheduler.on_transient_failure(now), RefreshReport::Failing);
        assert_eq!(scheduler.consecutive_failures(), 3);

        // A 4th failure does not re-report Failing.
        now = scheduler.due_at();
        assert_eq!(scheduler.action(now), RefreshAction::RefreshNow);
        assert_eq!(scheduler.on_transient_failure(now), RefreshReport::None);
    }

    #[test]
    fn success_after_failing_reports_recovered_and_resets_counters() {
        let mut scheduler = RefreshScheduler::new(epoch(), epoch() + Duration::hours(1));
        for _ in 0..3 {
            let now = scheduler.due_at();
            scheduler.action(now);
            scheduler.on_transient_failure(now);
        }
        assert_eq!(scheduler.consecutive_failures(), 3);

        let new_authorized_at = epoch() + Duration::hours(2);
        let new_expires_at = new_authorized_at + Duration::hours(1);
        let report = scheduler.on_success(new_authorized_at, new_expires_at);
        assert_eq!(report, RefreshReport::Recovered);
        assert_eq!(scheduler.consecutive_failures(), 0);
        assert_eq!(scheduler.due_at(), new_expires_at - Duration::minutes(5));
        assert!(!scheduler.is_in_flight());
    }

    #[test]
    fn success_with_no_prior_failures_reports_none() {
        let mut scheduler = RefreshScheduler::new(epoch(), epoch() + Duration::hours(1));
        let due = scheduler.due_at();
        scheduler.action(due);
        let report = scheduler.on_success(due, due + Duration::hours(1));
        assert_eq!(report, RefreshReport::None);
    }
}

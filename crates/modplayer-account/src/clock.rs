// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Clock` trait, `SystemClock`, `FakeClock` (data-model.md; Constitution
//! VIII — timing rules use `FakeClock::advance`, never `sleep`).

use std::sync::Mutex;

use time::{Duration, OffsetDateTime};

/// The current time, injected so refresh/expiry/backoff logic can be tested
/// without sleeping (Constitution VIII).
pub trait Clock: Send + Sync {
    fn now(&self) -> OffsetDateTime;
}

/// `Clock` backed by the real system clock.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl SystemClock {
    pub fn new() -> Self {
        Self
    }
}

impl Clock for SystemClock {
    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::now_utc()
    }
}

/// `Clock` test double: starts at a fixed instant and only moves when
/// `advance`/`set` is called.
#[derive(Debug)]
pub struct FakeClock {
    now: Mutex<OffsetDateTime>,
}

impl FakeClock {
    /// Start the clock at `start`.
    pub fn new(start: OffsetDateTime) -> Self {
        Self {
            now: Mutex::new(start),
        }
    }

    /// Move the clock forward by `duration`.
    pub fn advance(&self, duration: Duration) {
        let mut now = self.lock();
        *now += duration;
    }

    /// Jump the clock directly to `at`.
    pub fn set(&self, at: OffsetDateTime) {
        *self.lock() = at;
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, OffsetDateTime> {
        self.now
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl Default for FakeClock {
    /// Starts at the Unix epoch.
    fn default() -> Self {
        Self::new(OffsetDateTime::UNIX_EPOCH)
    }
}

impl Clock for FakeClock {
    fn now(&self) -> OffsetDateTime {
        *self.lock()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_clock_starts_at_the_given_instant() {
        let start = OffsetDateTime::UNIX_EPOCH + Duration::seconds(1000);
        let clock = FakeClock::new(start);
        assert_eq!(clock.now(), start);
    }

    #[test]
    fn advance_moves_the_clock_forward() {
        let clock = FakeClock::default();
        clock.advance(Duration::seconds(60));
        assert_eq!(
            clock.now(),
            OffsetDateTime::UNIX_EPOCH + Duration::seconds(60)
        );
    }

    #[test]
    fn set_jumps_the_clock_directly() {
        let clock = FakeClock::default();
        let target = OffsetDateTime::UNIX_EPOCH + Duration::days(1);
        clock.set(target);
        assert_eq!(clock.now(), target);
    }

    #[test]
    fn system_clock_returns_a_plausible_time() {
        // Sanity check only: the real clock must be well after the epoch.
        assert!(SystemClock::new().now() > OffsetDateTime::UNIX_EPOCH);
    }
}

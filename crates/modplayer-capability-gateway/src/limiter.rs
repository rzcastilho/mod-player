// SPDX-License-Identifier: MIT OR Apache-2.0

//! Per-plugin, per-category rate limiting (G5, FR-025, data-model.md
//! §1.6, research R13).

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::refusal::Refusal;

/// The window every category's rolling limit is measured over.
const WINDOW: Duration = Duration::from_secs(1);

/// The most admitted calls per category per rolling second (spec-fixed;
/// design note 15 — do not retune without recording a deviation).
pub const LIMIT: usize = 100;

/// The five rate-limit buckets (data-model.md §1.6). Every [`RequestKind`]
/// — including calls served locally without an RPC — is counted under
/// exactly one of these (G5).
///
/// [`RequestKind`]: crate::api::RequestKind
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RateCategory {
    Transport,
    Markers,
    Effects,
    State,
    Timers,
}

impl RateCategory {
    const ALL: [RateCategory; 5] = [
        RateCategory::Transport,
        RateCategory::Markers,
        RateCategory::Effects,
        RateCategory::State,
        RateCategory::Timers,
    ];

    const fn index(self) -> usize {
        match self {
            RateCategory::Transport => 0,
            RateCategory::Markers => 1,
            RateCategory::Effects => 2,
            RateCategory::State => 3,
            RateCategory::Timers => 4,
        }
    }
}

/// A per-plugin rolling rate limiter, one bucket per [`RateCategory`]
/// (G5). `admit` evicts timestamps older than the 1 s window before
/// counting, so old admissions never linger and the 101st call within the
/// window is refused without being recorded.
#[derive(Debug, Clone)]
pub struct RateLimiter {
    windows: [VecDeque<Instant>; 5],
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimiter {
    #[must_use]
    pub fn new() -> Self {
        Self {
            windows: std::array::from_fn(|_| VecDeque::new()),
        }
    }

    /// Evict entries older than `now - 1s` from every bucket (test/
    /// diagnostic helper; `admit` does this per-bucket lazily too).
    pub fn evict_all(&mut self, now: Instant) {
        for category in RateCategory::ALL {
            self.evict(category, now);
        }
    }

    fn evict(&mut self, category: RateCategory, now: Instant) {
        let bucket = &mut self.windows[category.index()];
        while let Some(&front) = bucket.front() {
            if now.saturating_duration_since(front) >= WINDOW {
                bucket.pop_front();
            } else {
                break;
            }
        }
    }

    /// Admit one call in `category` at `now`: evicts stale entries, then
    /// either records `now` and returns `Ok(())` (the 1st..=100th call in
    /// the window) or refuses without recording anything (the 101st+,
    /// `rate_limited`) — refused calls consume no quota (G2/G5).
    pub fn admit(&mut self, category: RateCategory, now: Instant) -> Result<(), Refusal> {
        self.evict(category, now);
        let bucket = &mut self.windows[category.index()];
        if bucket.len() >= LIMIT {
            return Err(Refusal::rate_limited());
        }
        bucket.push_back(now);
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;

    #[test]
    fn admits_up_to_limit_then_refuses() {
        let mut limiter = RateLimiter::new();
        let start = Instant::now();
        for _ in 0..LIMIT {
            assert!(limiter.admit(RateCategory::Transport, start).is_ok());
        }
        assert!(limiter.admit(RateCategory::Transport, start).is_err());
    }

    #[test]
    fn categories_are_independent() {
        let mut limiter = RateLimiter::new();
        let start = Instant::now();
        for _ in 0..LIMIT {
            limiter.admit(RateCategory::Transport, start).unwrap();
        }
        assert!(limiter.admit(RateCategory::Markers, start).is_ok());
    }

    #[test]
    fn window_slides() {
        let mut limiter = RateLimiter::new();
        let start = Instant::now();
        for _ in 0..LIMIT {
            limiter.admit(RateCategory::Transport, start).unwrap();
        }
        assert!(limiter.admit(RateCategory::Transport, start).is_err());
        let later = start + Duration::from_millis(1_001);
        assert!(limiter.admit(RateCategory::Transport, later).is_ok());
    }
}

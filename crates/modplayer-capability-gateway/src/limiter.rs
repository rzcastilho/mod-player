// SPDX-License-Identifier: MIT OR Apache-2.0

//! Per-plugin, per-category rate limiting (G5, FR-025, data-model.md
//! §1.6, research R13).

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::refusal::Refusal;
use crate::ui::limits::{NOTIFY_LIMIT, NOTIFY_WINDOW, UI_LIMIT, UI_WINDOW};

/// The window every original (009) category's rolling limit is measured
/// over.
const WINDOW: Duration = Duration::from_secs(1);

/// The most admitted calls per category per rolling second (spec-fixed;
/// design note 15 — do not retune without recording a deviation).
pub const LIMIT: usize = 100;

/// The seven rate-limit buckets (data-model.md §1.6, §1.8). Every
/// [`RequestKind`] — including calls served locally without an RPC — is
/// counted under exactly one of these (G5). `Ui`/`Notify`
/// (011-plugin-ui-contributions) each carry their own `(limit, window)`
/// pair rather than sharing the original five's fixed 100/1s (research
/// R2).
///
/// [`RequestKind`]: crate::api::RequestKind
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RateCategory {
    Transport,
    Markers,
    Effects,
    State,
    Timers,
    Ui,
    Notify,
}

impl RateCategory {
    const ALL: [RateCategory; 7] = [
        RateCategory::Transport,
        RateCategory::Markers,
        RateCategory::Effects,
        RateCategory::State,
        RateCategory::Timers,
        RateCategory::Ui,
        RateCategory::Notify,
    ];

    const fn index(self) -> usize {
        match self {
            RateCategory::Transport => 0,
            RateCategory::Markers => 1,
            RateCategory::Effects => 2,
            RateCategory::State => 3,
            RateCategory::Timers => 4,
            RateCategory::Ui => 5,
            RateCategory::Notify => 6,
        }
    }

    /// This bucket's own admitted-per-window cap.
    #[must_use]
    pub const fn limit(self) -> usize {
        match self {
            RateCategory::Notify => NOTIFY_LIMIT,
            RateCategory::Ui => UI_LIMIT,
            _ => LIMIT,
        }
    }

    /// This bucket's own rolling window length.
    #[must_use]
    pub const fn window(self) -> Duration {
        match self {
            RateCategory::Notify => NOTIFY_WINDOW,
            RateCategory::Ui => UI_WINDOW,
            _ => WINDOW,
        }
    }
}

/// A per-plugin rolling rate limiter, one bucket per [`RateCategory`]
/// (G5). `admit` evicts timestamps older than the bucket's own window
/// before counting, so old admissions never linger and a call past the
/// bucket's limit within the window is refused without being recorded.
#[derive(Debug, Clone)]
pub struct RateLimiter {
    windows: [VecDeque<Instant>; 7],
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
        let window = category.window();
        let bucket = &mut self.windows[category.index()];
        while let Some(&front) = bucket.front() {
            if now.saturating_duration_since(front) >= window {
                bucket.pop_front();
            } else {
                break;
            }
        }
    }

    /// Admit one call in `category` at `now`: evicts stale entries, then
    /// either records `now` and returns `Ok(())` (within `category.
    /// limit()` calls in `category.window()`) or refuses without
    /// recording anything (`rate_limited`) — refused calls consume no
    /// quota (G2/G5).
    pub fn admit(&mut self, category: RateCategory, now: Instant) -> Result<(), Refusal> {
        self.evict(category, now);
        let limit = category.limit();
        let bucket = &mut self.windows[category.index()];
        if bucket.len() >= limit {
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

// SPDX-License-Identifier: MIT OR Apache-2.0

//! `TimerSet`: a plugin's own `set_timeout`/`set_interval`/
//! `schedule_at_position` timers (RT9, FR-022, data-model.md §2).

use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

/// The most timers (all kinds combined) a plugin may hold pending at once
/// (FR-022, spec-fixed).
pub const MAX_TIMERS: usize = 256;

/// The shortest interval/timeout a plugin may request.
pub const MIN_INTERVAL: Duration = Duration::from_millis(1);

/// A timer's identity within its plugin, stable until it fires (`timeout`)
/// or is cleared (data-model.md §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TimerHandle(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TimerKind {
    Timeout,
    Interval(Duration),
    AtPosition(u64),
}

/// A plugin's pending timers (RT9). `set_timeout`/`set_interval` are kept
/// in a `deadline -> handles` map for O(log n) "what's next"; position
/// timers are checked against every position sample instead, since they
/// have no wall-clock deadline of their own.
#[derive(Debug, Default)]
pub struct TimerSet {
    next_handle: u32,
    /// `Timeout`/`Interval` timers, keyed by their next wake instant.
    deadlines: BTreeMap<Instant, Vec<TimerHandle>>,
    kinds: HashMap<TimerHandle, TimerKind>,
}

impl TimerSet {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.kinds.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }

    fn alloc(&mut self) -> TimerHandle {
        self.next_handle += 1;
        TimerHandle(self.next_handle)
    }

    /// `set_timeout`/`set_interval` (contract §3): `None` (`timer_limit`)
    /// beyond [`MAX_TIMERS`] or below [`MIN_INTERVAL`] (`invalid_argument`
    /// is the caller's job to distinguish — this returns `None` for both,
    /// the binding maps the reason from the input it already validated).
    pub fn schedule(&mut self, delay: Duration, repeat: bool, now: Instant) -> Option<TimerHandle> {
        if self.kinds.len() >= MAX_TIMERS || delay < MIN_INTERVAL {
            return None;
        }
        let handle = self.alloc();
        let kind = if repeat {
            TimerKind::Interval(delay)
        } else {
            TimerKind::Timeout
        };
        self.kinds.insert(handle, kind);
        self.deadlines.entry(now + delay).or_default().push(handle);
        Some(handle)
    }

    /// `schedule_at_position` (contract §3): fires once a position sample
    /// is `>= target`.
    pub fn schedule_at_position(&mut self, target_ms: u64) -> Option<TimerHandle> {
        if self.kinds.len() >= MAX_TIMERS {
            return None;
        }
        let handle = self.alloc();
        self.kinds.insert(handle, TimerKind::AtPosition(target_ms));
        Some(handle)
    }

    /// `clear(handle)`: `false` (`not_found`) for an unknown or
    /// already-fired handle.
    pub fn clear(&mut self, handle: TimerHandle) -> bool {
        if self.kinds.remove(&handle).is_none() {
            return false;
        }
        for handles in self.deadlines.values_mut() {
            handles.retain(|h| *h != handle);
        }
        self.deadlines.retain(|_, handles| !handles.is_empty());
        true
    }

    /// The next wall-clock deadline among pending `Timeout`/`Interval`
    /// timers, for the scheduler's `recv_timeout` wait (RT6). `None` if
    /// none are pending (position timers have no deadline of their own).
    #[must_use]
    pub fn next_deadline(&self) -> Option<Instant> {
        self.deadlines.keys().next().copied()
    }

    /// Pop every `Timeout`/`Interval` timer due at or before `now`,
    /// re-arming `Interval`s for their next fire. Returns the handles to
    /// deliver as `timer` events, in no particular cross-handle order
    /// (each is delivered once).
    pub fn drain_due(&mut self, now: Instant) -> Vec<TimerHandle> {
        let mut due = Vec::new();
        let ready_keys: Vec<Instant> = self.deadlines.range(..=now).map(|(k, _)| *k).collect();
        for key in ready_keys {
            if let Some(handles) = self.deadlines.remove(&key) {
                for handle in handles {
                    match self.kinds.get(&handle).copied() {
                        Some(TimerKind::Timeout) => {
                            self.kinds.remove(&handle);
                            due.push(handle);
                        }
                        Some(TimerKind::Interval(period)) => {
                            due.push(handle);
                            self.deadlines.entry(now + period).or_default().push(handle);
                        }
                        Some(TimerKind::AtPosition(_)) | None => {}
                    }
                }
            }
        }
        due
    }

    /// Called on every position sample (RT9): returns every
    /// `schedule_at_position` handle whose target has been reached (fires
    /// once, then is removed), paired with the sampled position that
    /// crossed it.
    pub fn check_position(&mut self, position_ms: u64) -> Vec<TimerHandle> {
        let mut fired = Vec::new();
        self.kinds.retain(|handle, kind| {
            if let TimerKind::AtPosition(target) = kind
                && position_ms >= *target
            {
                fired.push(*handle);
                return false;
            }
            true
        });
        fired
    }

    /// RT9: a `track_generation` change cancels every position timer
    /// silently (no `not_found`, no event).
    pub fn cancel_position_timers(&mut self) {
        self.kinds
            .retain(|_, kind| !matches!(kind, TimerKind::AtPosition(_)));
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;

    #[test]
    fn timeout_fires_once() {
        let mut timers = TimerSet::new();
        let now = Instant::now();
        let handle = timers
            .schedule(Duration::from_millis(10), false, now)
            .expect("scheduled");
        assert!(timers.drain_due(now).is_empty());
        let due = timers.drain_due(now + Duration::from_millis(11));
        assert_eq!(due, vec![handle]);
        assert!(timers.is_empty());
    }

    #[test]
    fn interval_rearms() {
        let mut timers = TimerSet::new();
        let now = Instant::now();
        let handle = timers
            .schedule(Duration::from_millis(10), true, now)
            .expect("scheduled");
        let due1 = timers.drain_due(now + Duration::from_millis(11));
        assert_eq!(due1, vec![handle]);
        let due2 = timers.drain_due(now + Duration::from_millis(22));
        assert_eq!(due2, vec![handle]);
    }

    #[test]
    fn clear_unknown_is_not_found() {
        let mut timers = TimerSet::new();
        assert!(!timers.clear(TimerHandle(999)));
    }

    #[test]
    fn limit_257th_refused() {
        let mut timers = TimerSet::new();
        let now = Instant::now();
        for _ in 0..MAX_TIMERS {
            timers
                .schedule(Duration::from_secs(60), false, now)
                .expect("under limit");
        }
        assert!(
            timers
                .schedule(Duration::from_secs(60), false, now)
                .is_none()
        );
    }

    #[test]
    fn position_timer_fires_once_at_or_past_target() {
        let mut timers = TimerSet::new();
        let handle = timers.schedule_at_position(5_000).expect("scheduled");
        assert!(timers.check_position(4_999).is_empty());
        assert_eq!(timers.check_position(5_000), vec![handle]);
        assert!(timers.check_position(5_001).is_empty());
    }

    #[test]
    fn cancel_position_timers_clears_only_those() {
        let mut timers = TimerSet::new();
        let now = Instant::now();
        let position_handle = timers.schedule_at_position(5_000).expect("scheduled");
        let timeout_handle = timers
            .schedule(Duration::from_millis(10), false, now)
            .expect("scheduled");
        timers.cancel_position_timers();
        assert!(timers.check_position(5_000).is_empty());
        assert_eq!(
            timers.drain_due(now + Duration::from_millis(11)),
            vec![timeout_handle]
        );
        let _ = position_handle;
    }
}

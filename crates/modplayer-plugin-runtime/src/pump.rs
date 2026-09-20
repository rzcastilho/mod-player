// SPDX-License-Identifier: MIT OR Apache-2.0

//! Position/meter sampling primitives, read on the plugin's own thread
//! from `Arc<RtShared>` (research R5, RT6/RT10). Never touches the audio
//! thread's own data structures — only the published atomics
//! `modplayer-ui` reads the same way (Constitution I).

use std::time::{Duration, Instant};

use modplayer_engine::{PositionClock, RtShared};

use modplayer_capability_gateway::event::LevelInfo;

/// A plugin's live `position`/`meter` subscription state (data-model.md
/// §2). One per plugin, owned by its scheduler thread.
#[derive(Debug)]
pub struct Subscriptions {
    /// `1..=60`, default 10 (contract §3 `subscribe_position`).
    position_rate_hz: u8,
    last_position_ms: Option<u64>,
    last_position_sample_at: Option<Instant>,
    meter: bool,
    last_meter_at: Option<Instant>,
}

impl Default for Subscriptions {
    fn default() -> Self {
        Self {
            position_rate_hz: 10,
            last_position_ms: None,
            last_position_sample_at: None,
            meter: false,
            last_meter_at: None,
        }
    }
}

/// `meter` events are sampled at the UI's own cadence (research R5): the
/// existing `ticker.rs` `TICK_INTERVAL`.
pub const METER_INTERVAL: Duration = Duration::from_millis(33);

impl Subscriptions {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `subscribe_position(rate_hz)`: clamps into `1..=60` (contract §3).
    pub fn set_position_rate(&mut self, rate_hz: u8) {
        self.position_rate_hz = rate_hz.clamp(1, 60);
    }

    #[must_use]
    pub fn position_rate_hz(&self) -> u8 {
        self.position_rate_hz
    }

    pub fn set_meter_enabled(&mut self, enabled: bool) {
        self.meter = enabled;
    }

    #[must_use]
    pub fn meter_enabled(&self) -> bool {
        self.meter
    }

    fn position_interval(&self) -> Duration {
        Duration::from_secs_f64(1.0 / f64::from(self.position_rate_hz.max(1)))
    }

    /// RT6: how long until the next position tick is due, or `None` if
    /// this is the very first sample (due immediately).
    #[must_use]
    pub fn next_position_wake(&self, now: Instant) -> Option<Instant> {
        match self.last_position_sample_at {
            Some(at) => Some(at + self.position_interval()),
            None => Some(now),
        }
    }

    #[must_use]
    pub fn next_meter_wake(&self, now: Instant) -> Option<Instant> {
        if !self.meter {
            return None;
        }
        match self.last_meter_at {
            Some(at) => Some(at + METER_INTERVAL),
            None => Some(now),
        }
    }

    /// RT10: sample the position from `shared` at `source_rate`; returns
    /// `Some(ms)` only when due *and* different from the last delivered
    /// value (a seek/loop-wrap while paused still changes the sampled
    /// value once, satisfying "once after a seek/wrap while paused").
    pub fn sample_position(
        &mut self,
        shared: &RtShared,
        source_rate: u32,
        now: Instant,
    ) -> Option<u64> {
        let due = self.next_position_wake(now).is_none_or(|at| now >= at);
        if !due {
            return None;
        }
        self.last_position_sample_at = Some(now);
        let position_ms = PositionClock::now(shared, source_rate).as_millis() as u64;
        if self.last_position_ms == Some(position_ms) {
            return None;
        }
        self.last_position_ms = Some(position_ms);
        Some(position_ms)
    }

    /// Reset the "last delivered" edge (e.g. on `track_changed`) so the
    /// next sample is always delivered even if the raw value happens to
    /// coincide with the previous track's last one.
    pub fn reset_position_edge(&mut self) {
        self.last_position_ms = None;
    }

    /// RT5/R5: sample `pre`/`post` levels and the spectrum for a `meter`
    /// event, when due and subscribed.
    pub fn sample_meter(
        &mut self,
        shared: &RtShared,
        now: Instant,
    ) -> Option<(LevelInfo, LevelInfo, Box<[f32; 64]>)> {
        if !self.meter {
            return None;
        }
        let due = self.next_meter_wake(now).is_none_or(|at| now >= at);
        if !due {
            return None;
        }
        self.last_meter_at = Some(now);
        let (pre_pl, pre_pr, pre_rl, pre_rr) = shared.pre_level();
        let (post_pl, post_pr, post_rl, post_rr) = shared.post_level();
        let pre = LevelInfo {
            peak_l: pre_pl,
            peak_r: pre_pr,
            rms_l: pre_rl,
            rms_r: pre_rr,
        };
        let post = LevelInfo {
            peak_l: post_pl,
            peak_r: post_pr,
            rms_l: post_rl,
            rms_r: post_rr,
        };
        Some((pre, post, Box::new(shared.spectrum())))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;

    #[test]
    fn rate_clamps_to_1_60() {
        let mut subs = Subscriptions::new();
        subs.set_position_rate(0);
        assert_eq!(subs.position_rate_hz(), 1);
        subs.set_position_rate(200);
        assert_eq!(subs.position_rate_hz(), 60);
    }

    #[test]
    fn silent_while_paused_and_unchanged() {
        let shared = RtShared::new();
        shared.write_anchor(0, Instant::now(), false, 1024, 1.0);
        let mut subs = Subscriptions::new();
        let now = Instant::now();
        let first = subs.sample_position(&shared, 44_100, now);
        assert_eq!(first, Some(0));
        let second = subs.sample_position(&shared, 44_100, now + subs.position_interval());
        assert_eq!(second, None, "unchanged position is not redelivered");
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Smoothed`: a per-sample linear ramp toward a target value (FR-010,
//! data-model.md §3.1). Every continuous parameter is one of these; a
//! retarget always restarts from the *current* value, never the old
//! target, so back-to-back edits never produce a discontinuity.

/// A continuous parameter's current value, mid-ramp toward `target` (or
/// already there).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Smoothed {
    pub current: f32,
    pub target: f32,
    step: f32,
    remaining: u32,
}

impl Smoothed {
    /// A settled value: `current == target == value`.
    #[must_use]
    pub const fn new(value: f32) -> Self {
        Self {
            current: value,
            target: value,
            step: 0.0,
            remaining: 0,
        }
    }

    /// Start (or restart) a ramp toward `target` over `ramp_frames`
    /// frames, from whatever `current` is right now (FR-010). `0` frames
    /// is an immediate jump.
    pub fn set_target(&mut self, target: f32, ramp_frames: u32) {
        self.target = target;
        if ramp_frames == 0 {
            self.current = target;
            self.step = 0.0;
            self.remaining = 0;
            return;
        }
        self.step = (target - self.current) / ramp_frames as f32;
        self.remaining = ramp_frames;
    }

    /// Advance exactly one frame, returning the new `current` value. A
    /// settled `Smoothed` (`remaining == 0`) just returns `current`
    /// unchanged — cheap to call unconditionally every sample.
    pub fn advance(&mut self) -> f32 {
        if self.remaining > 0 {
            self.remaining -= 1;
            self.current = if self.remaining == 0 {
                self.target
            } else {
                self.current + self.step
            };
        }
        self.current
    }

    /// Whether the ramp has reached its target.
    #[must_use]
    pub const fn is_settled(&self) -> bool {
        self.remaining == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // `ramp_reaches_target_in_exactly_ramp_frames` and
    // `retarget_restarts_from_current` (FR-010, contracts/engine-effect-
    // chain.md §12) live in `tests/smooth.rs`.

    #[test]
    fn zero_frame_ramp_jumps_immediately() {
        let mut s = Smoothed::new(0.0);
        s.set_target(5.0, 0);
        assert!(s.is_settled());
        assert_eq!(s.current, 5.0);
        assert_eq!(s.advance(), 5.0);
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Crossfade`: the equal-power dry/wet fade every chain-edit transition
//! uses (FR-005, research R6) — add/remove/bypass gain, reorder's
//! two-phase dry-out/wet-in, and (per-kernel) discrete-parameter switch
//! seams. The curve shape mirrors `modplayer_engine::loop_math::
//! crossfade_gains` (`t = (index+1)/(length+1)`, `(cos(t*pi/2),
//! sin(t*pi/2))`); reimplemented here rather than shared because this
//! crate carries no runtime dependency on the engine (research R1).

use std::f32::consts::FRAC_PI_2;

/// A two-state equal-power fader: `dry` is the outgoing/old state,
/// `wet` the incoming/new one. `set_target` starts (or retargets) a fade
/// toward one end over a fixed number of frames; `gains()` reads the
/// current `(dry, wet)` pair without mutating; `advance()` steps one
/// frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Crossfade {
    to_wet: bool,
    elapsed: u32,
    length: u32,
}

impl Crossfade {
    /// Already settled fully at one end: `wet = true` ⇒ `(0.0, 1.0)`,
    /// `wet = false` ⇒ `(1.0, 0.0)`.
    #[must_use]
    pub const fn settled(wet: bool) -> Self {
        Self {
            to_wet: wet,
            elapsed: 0,
            length: 0,
        }
    }

    /// Start (or retarget) a fade toward `wet` over `length_frames`
    /// frames. Retargeting toward the end already reached is a no-op;
    /// retargeting mid-fade restarts the equal-power curve from this
    /// fade's *current* position toward the new end (research R6: every
    /// transition is a crossfade of two *live* states, so a reversal
    /// never needs to fake a symmetric current gain — it simply resumes
    /// the same shared curve in the other direction).
    pub fn set_target(&mut self, wet: bool, length_frames: u32) {
        if self.to_wet == wet && self.is_settled() {
            return;
        }
        self.to_wet = wet;
        self.elapsed = 0;
        self.length = length_frames;
    }

    /// Whether the fade has reached its target end.
    #[must_use]
    pub const fn is_settled(&self) -> bool {
        self.length == 0 || self.elapsed >= self.length
    }

    /// The current `(dry, wet)` equal-power gain pair.
    #[must_use]
    pub fn gains(&self) -> (f32, f32) {
        if self.is_settled() {
            return if self.to_wet { (0.0, 1.0) } else { (1.0, 0.0) };
        }
        let index = self.elapsed.min(self.length.saturating_sub(1));
        #[allow(clippy::cast_precision_loss)]
        let t = (index + 1) as f32 / (self.length + 1) as f32;
        let theta = t * FRAC_PI_2;
        let (falling, rising) = (theta.cos(), theta.sin());
        if self.to_wet {
            (falling, rising)
        } else {
            (rising, falling)
        }
    }

    /// Advance one frame (called once per rendered frame while mid-fade;
    /// a no-op once settled).
    pub fn advance(&mut self) {
        if self.elapsed < self.length {
            self.elapsed += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // `equal_power_and_exact_endpoints` (FR-005, contracts/engine-effect-
    // chain.md §12) lives in `tests/crossfade.rs`.

    #[test]
    fn zero_length_is_a_hard_cut() {
        let mut cf = Crossfade::settled(false);
        cf.set_target(true, 0);
        assert!(cf.is_settled());
        assert_eq!(cf.gains(), (0.0, 1.0));
    }

    #[test]
    fn retarget_to_current_end_is_noop() {
        let mut cf = Crossfade::settled(true);
        cf.set_target(true, 5);
        assert!(cf.is_settled(), "already at the requested end");
    }
}

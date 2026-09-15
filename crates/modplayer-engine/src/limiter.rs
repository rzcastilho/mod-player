// SPDX-License-Identifier: MIT OR Apache-2.0

//! The non-bypassable sample-peak limiter (FR-009, research R8): a
//! zero-lookahead brickwall clamp, `y = clamp(x, -ceiling_lin, +ceiling_lin)`.
//! Hard-wired into `Processor::render`; no `Command` can remove or raise it
//! above `CeilingDb::MAX`.

use crate::types::CeilingDb;

/// A sample-peak brickwall limiter with a recomputable ceiling.
#[derive(Debug, Clone, Copy)]
pub struct Limiter {
    ceiling_lin: f32,
}

impl Limiter {
    /// Construct a limiter for `ceiling`.
    pub fn new(ceiling: CeilingDb) -> Self {
        Self {
            ceiling_lin: ceiling.to_linear(),
        }
    }

    /// Recompute the ceiling from a (re-clamped) `CeilingDb` — the command
    /// handler re-clamps on top of the type's own clamp as defence in depth
    /// (FR-010).
    pub fn set_ceiling(&mut self, ceiling: CeilingDb) {
        self.ceiling_lin = CeilingDb::new(ceiling.db()).to_linear();
    }

    /// The current ceiling as a linear amplitude.
    pub fn ceiling_lin(&self) -> f32 {
        self.ceiling_lin
    }

    /// Clamp every sample in `buf` to `[-ceiling_lin, +ceiling_lin]` in place.
    pub fn process(&self, buf: &mut [f32]) {
        for sample in buf.iter_mut() {
            *sample = sample.clamp(-self.ceiling_lin, self.ceiling_lin);
        }
    }
}

impl Default for Limiter {
    fn default() -> Self {
        Self::new(CeilingDb::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_to_ceiling() {
        let limiter = Limiter::new(CeilingDb::new(-6.0));
        let mut buf = [1.0f32, -1.0, 0.0, 0.3];
        limiter.process(&mut buf);
        let ceiling = CeilingDb::new(-6.0).to_linear();
        for sample in buf {
            assert!(sample.abs() <= ceiling + 1e-6);
        }
        assert!((buf[2]).abs() < 1e-9);
    }

    #[test]
    fn set_ceiling_reclamps_out_of_range_input() {
        let mut limiter = Limiter::default();
        limiter.set_ceiling(CeilingDb::new(10.0)); // out of range, clamped to -0.1
        let expected = CeilingDb::new(CeilingDb::MAX).to_linear();
        assert!((limiter.ceiling_lin() - expected).abs() < 1e-6);
    }
}

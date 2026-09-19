// SPDX-License-Identifier: MIT OR Apache-2.0

//! `CostRing`: a fixed-capacity ring of per-callback cost percentages
//! (FR-012a), the raw material for both a node's rolling-mean cost and
//! the whole-render overload window (research R10). Allocation-free after
//! construction: a plain `[f32; N]` array, never resized.

use crate::consts::COST_RING_CAPACITY;

/// A ring buffer of the last `COST_RING_CAPACITY` (≈ 1 s of callbacks)
/// percentage readings, plus a running sum so `mean()` is O(1).
#[derive(Debug, Clone, Copy)]
pub struct CostRing {
    samples: [f32; COST_RING_CAPACITY],
    write: usize,
    len: usize,
    sum: f32,
}

impl CostRing {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            samples: [0.0; COST_RING_CAPACITY],
            write: 0,
            len: 0,
            sum: 0.0,
        }
    }

    /// Record one callback's percentage-of-period reading, evicting the
    /// oldest sample once the ring is full.
    pub fn push(&mut self, pct: f32) {
        if self.len == COST_RING_CAPACITY {
            self.sum -= self.samples[self.write];
        } else {
            self.len += 1;
        }
        self.samples[self.write] = pct;
        self.sum += pct;
        self.write = (self.write + 1) % COST_RING_CAPACITY;
    }

    /// The rolling mean over every sample currently held; `0.0` when
    /// empty (a slot with no renders yet reads as zero cost).
    #[must_use]
    pub fn mean(&self) -> f32 {
        if self.len == 0 {
            0.0
        } else {
            self.sum / self.len as f32
        }
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Discard every reading (a slot's cost resets when it is freed and
    /// later reactivated by a new node).
    pub fn clear(&mut self) {
        self.len = 0;
        self.write = 0;
        self.sum = 0.0;
    }
}

impl Default for CostRing {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mean_of_empty_ring_is_zero() {
        assert_eq!(CostRing::new().mean(), 0.0);
    }

    #[test]
    fn mean_tracks_pushed_values() {
        let mut ring = CostRing::new();
        ring.push(10.0);
        ring.push(20.0);
        ring.push(30.0);
        assert!((ring.mean() - 20.0).abs() < 1e-6);
        assert_eq!(ring.len(), 3);
    }

    #[test]
    fn ring_evicts_oldest_past_capacity() {
        let mut ring = CostRing::new();
        for _ in 0..COST_RING_CAPACITY {
            ring.push(0.0);
        }
        assert_eq!(ring.len(), COST_RING_CAPACITY);
        ring.push(100.0);
        assert_eq!(ring.len(), COST_RING_CAPACITY);
        // Only one 100.0 sample among COST_RING_CAPACITY zeros.
        assert!((ring.mean() - 100.0 / COST_RING_CAPACITY as f32).abs() < 1e-4);
    }
}

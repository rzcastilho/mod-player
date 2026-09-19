// SPDX-License-Identifier: MIT OR Apache-2.0

//! contracts/engine-effect-chain.md §12: catalog proptests — pins FR-006,
//! FR-010, SC-005.

use modplayer_effects::catalog::{self, NodeKind, ParamShape};
use proptest::prelude::*;

fn effective_max(shape: ParamShape, source_rate: u32) -> Option<(f32, f32)> {
    match shape {
        ParamShape::Continuous {
            min,
            max,
            nyquist_clamped,
        } => {
            let max = if nyquist_clamped {
                max.min(0.45 * source_rate as f32)
            } else {
                max
            };
            Some((min, max.max(min)))
        }
        ParamShape::Discrete { .. } => None,
    }
}

proptest! {
    /// `clamp` is idempotent, stays in range, and is monotone
    /// non-decreasing in its input — for every parameter of every kind.
    #[test]
    fn clamp_is_idempotent_in_range_and_monotone(
        kind_idx in 0usize..NodeKind::ALL.len(),
        value_a in -100_000f32..100_000f32,
        value_b in -100_000f32..100_000f32,
        source_rate in 8_000u32..192_000u32,
    ) {
        let kind = NodeKind::ALL[kind_idx];
        let (lo, hi) = if value_a <= value_b { (value_a, value_b) } else { (value_b, value_a) };
        for def in catalog::params(kind) {
            let clamped_lo = catalog::clamp(kind, def.id, lo, source_rate);
            let clamped_hi = catalog::clamp(kind, def.id, hi, source_rate);
            prop_assert!(clamped_lo <= clamped_hi + 1e-3, "monotone: {clamped_lo} > {clamped_hi}");

            let twice = catalog::clamp(kind, def.id, clamped_lo, source_rate);
            prop_assert!((twice - clamped_lo).abs() < 1e-3, "idempotent: {twice} != {clamped_lo}");

            match effective_max(def.shape, source_rate) {
                Some((min, max)) => {
                    prop_assert!(clamped_lo >= min - 1e-3 && clamped_lo <= max + 1e-3);
                }
                None => {
                    if let ParamShape::Discrete { count } = def.shape {
                        let max_index = f32::from(count.saturating_sub(1));
                        prop_assert!(clamped_lo >= 0.0 && clamped_lo <= max_index);
                    }
                }
            }
        }
    }

    /// The Nyquist clamp tracks `source_rate`: raising the rate never
    /// lowers the effective max, and a value far above range clamps to
    /// exactly `min(max, 0.45 * source_rate)`.
    #[test]
    fn nyquist_clamp_tracks_rate(
        rate_a in 8_000u32..96_000,
        rate_delta in 0u32..96_000,
    ) {
        let rate_b = rate_a + rate_delta;
        for kind in NodeKind::ALL {
            for def in catalog::params(kind) {
                if let ParamShape::Continuous { max, nyquist_clamped: true, .. } = def.shape {
                    let big = 1_000_000.0;
                    let clamped_a = catalog::clamp(kind, def.id, big, rate_a);
                    let clamped_b = catalog::clamp(kind, def.id, big, rate_b);
                    prop_assert!(clamped_b >= clamped_a - 1e-3, "raising rate must not lower the effective max");
                    let expected_a = max.min(0.45 * rate_a as f32);
                    prop_assert!((clamped_a - expected_a).abs() < 1e-2, "clamped={clamped_a} expected={expected_a}");
                }
            }
        }
    }

    /// Every kind/parameter's catalog default sits inside its own range
    /// at a representative source rate (SC-013: every default is
    /// transparent).
    #[test]
    fn defaults_are_in_range(kind_idx in 0usize..NodeKind::ALL.len()) {
        let kind = NodeKind::ALL[kind_idx];
        for def in catalog::params(kind) {
            match def.shape {
                ParamShape::Continuous { min, max, nyquist_clamped } => {
                    let max = if nyquist_clamped { max.min(0.45 * 44_100.0) } else { max };
                    prop_assert!(def.default >= min && def.default <= max, "{kind:?} {def:?}");
                }
                ParamShape::Discrete { count } => {
                    prop_assert!(def.default >= 0.0 && def.default <= f32::from(count.saturating_sub(1)));
                }
            }
            // The default must itself be a fixed point of `clamp`.
            let clamped = catalog::clamp(kind, def.id, def.default, 44_100);
            prop_assert!((clamped - def.default).abs() < 1e-3, "default not stable under clamp: {clamped} != {}", def.default);
        }
    }
}

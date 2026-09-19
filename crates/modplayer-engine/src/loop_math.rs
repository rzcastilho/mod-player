// SPDX-License-Identifier: MIT OR Apache-2.0

//! Pure loop-region math: crossfade sizing, position wrapping and
//! crossfade gain curves (006, contracts/engine-loop.md §5).

use std::f32::consts::FRAC_PI_2;

/// The crossfade actually usable for a region `[a, b)` at `configured`
/// frames (data-model.md §1.6, contracts/engine-loop.md §5): never wider
/// than the region itself (`b - a`), and never reaching before frame 0
/// (`a`).
///
/// ```
/// use modplayer_engine::loop_math::effective_crossfade;
/// assert_eq!(effective_crossfade(2_205, 3_000, 6_000), 2_205);
/// assert_eq!(effective_crossfade(2_205, 5_000, 5_200), 200); // region-limited
/// assert_eq!(effective_crossfade(2_205, 50, 4_000), 50); // A-limited
/// ```
pub fn effective_crossfade(configured: u64, a: u64, b: u64) -> u64 {
    configured.min(b.saturating_sub(a)).min(a)
}

/// The source-frame position after wrapping `elapsed` source-frames of
/// play across a `[a, b)` loop (data-model.md §1.6, contracts/
/// engine-loop.md §5): `a + (elapsed - a) % (b - a)` once `elapsed >= a`;
/// `elapsed` unchanged (not yet inside the loop, or a degenerate
/// `b <= a`) otherwise.
///
/// ```
/// use modplayer_engine::loop_math::wrap_position;
/// assert_eq!(wrap_position(100, 500, 100 + 3 * 400 + 37), 137);
/// assert_eq!(wrap_position(100, 500, 50), 50); // before A: unchanged
/// ```
pub fn wrap_position(a: u64, b: u64, elapsed: u64) -> u64 {
    if elapsed < a || b <= a {
        return elapsed;
    }
    let period = b - a;
    a + (elapsed - a) % period
}

/// The equal-power crossfade gains `(outgoing, incoming)` for seam frame
/// `index` (0-based) of an `x`-frame seam (data-model.md §1.6, contracts/
/// engine-loop.md §5): `t = (index + 1) / (x + 1)`,
/// `(cos(t*pi/2), sin(t*pi/2))`. `x == 0` is a hard cut: `(1.0, 0.0)`,
/// there being no seam to mix.
///
/// ```
/// use modplayer_engine::loop_math::crossfade_gains;
/// assert_eq!(crossfade_gains(0, 0), (1.0, 0.0));
/// let (out, inn) = crossfade_gains(4, 9); // midpoint of a 9-frame seam
/// assert!((out - inn).abs() < 1e-6);
/// ```
pub fn crossfade_gains(index: u64, x: u64) -> (f32, f32) {
    if x == 0 {
        return (1.0, 0.0);
    }
    let index = index.min(x.saturating_sub(1));
    #[allow(clippy::cast_precision_loss)]
    let t = (index + 1) as f32 / (x + 1) as f32;
    let theta = t * FRAC_PI_2;
    (theta.cos(), theta.sin())
}

/// The published position after subtracting a chain lead of `lead`
/// source frames from `raw` (the source's raw position), while a loop
/// `[a, b)` is active and `raw >= a` (008, research R7, contracts/
/// engine-effect-chain.md §9): `a + ((raw - a) - lead) mod (b - a)` —
/// the smallest non-negative representative, computed via wraparound
/// modular arithmetic rather than literally searching for `k`. Reduces
/// to 006's `b - (lead - (raw - a))` whenever `lead - (raw - a) < b - a`
/// (a lead shorter than one loop period). Degenerate `b <= a` falls back
/// to `raw.saturating_sub(lead)` (no loop to rewind within); the caller
/// (`Processor::render`) is responsible for the `raw >= a` precondition
/// (contracts/engine-effect-chain.md §9's `if loop active && raw >= a`
/// guard) — this function itself still behaves sanely (via `raw - a`
/// saturating to `0`) if that precondition is violated.
///
/// ```
/// use modplayer_engine::loop_math::rewind_in_loop;
/// // A 25-frame lead inside a [1_000, 1_100) loop, well within one period.
/// assert_eq!(rewind_in_loop(1_050, 25, 1_000, 1_100), 1_025);
/// // A lead that wraps past `a` lands back near `b`.
/// assert_eq!(rewind_in_loop(1_010, 25, 1_000, 1_100), 1_085);
/// ```
pub fn rewind_in_loop(raw: u64, lead: u64, a: u64, b: u64) -> u64 {
    if b <= a {
        return raw.saturating_sub(lead);
    }
    let period = b - a;
    let offset = raw.saturating_sub(a) % period;
    let lead_mod = lead % period;
    let diff = (offset + period - lead_mod) % period;
    a + diff
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// contracts/engine-loop.md §5: `effective_crossfade <= min(configured, b - a, a)`.
        #[test]
        fn effective_crossfade_never_exceeds_bounds(
            configured in 0u64..100_000,
            a in 0u64..1_000_000,
            len in 1u64..1_000_000,
        ) {
            let b = a + len;
            let x = effective_crossfade(configured, a, b);
            prop_assert!(x <= configured);
            prop_assert!(x <= b - a);
            prop_assert!(x <= a);
        }

        /// contracts/engine-loop.md §5: `out^2 + in^2 ~= 1` (equal power).
        #[test]
        fn crossfade_gains_preserve_power(index in 0u64..1_000, x in 0u64..1_000) {
            let (out, inn) = crossfade_gains(index, x);
            let power = out * out + inn * inn;
            prop_assert!((power - 1.0).abs() <= 1e-5, "power = {power}");
        }

        /// contracts/engine-loop.md §5:
        /// `wrap_position(a, b, a + k*(b-a) + r) == a + r` for `r < b - a`.
        #[test]
        fn wrap_position_is_periodic(
            a in 0u64..1_000_000,
            len in 1u64..100_000,
            k in 0u64..1_000,
            r_raw in 0u64..100_000,
        ) {
            let b = a + len;
            let r = r_raw % len;
            let elapsed = a + k * len + r;
            prop_assert_eq!(wrap_position(a, b, elapsed), a + r);
        }

        /// Before `a`, `wrap_position` is a no-op (nothing to wrap yet).
        #[test]
        fn wrap_position_before_a_is_unchanged(
            a in 1u64..1_000_000,
            len in 1u64..100_000,
            before in 0u64..1_000_000,
        ) {
            let elapsed = a.saturating_sub(1).min(before);
            prop_assert_eq!(wrap_position(a, a + len, elapsed), elapsed);
        }

        /// 008, research R7: `rewind_in_loop`'s result always lands inside
        /// `[a, b)`, for any `raw >= a` and any `lead` (including one
        /// longer than the loop period itself).
        #[test]
        fn rewind_in_loop_lands_in_region(
            a in 0u64..1_000_000,
            len in 1u64..100_000,
            extra in 0u64..500_000,
            lead in 0u64..1_000_000,
        ) {
            let b = a + len;
            let raw = a + extra;
            let published = rewind_in_loop(raw, lead, a, b);
            prop_assert!(published >= a && published < b, "published={published} a={a} b={b}");
        }

        /// A lead shorter than one loop period reduces to 006's original
        /// "republish through B" rule.
        #[test]
        fn rewind_in_loop_matches_006_when_lead_fits_one_period(
            a in 0u64..1_000_000,
            len in 2u64..100_000,
            offset_raw in 0u64..100_000,
            deficit_raw in 1u64..100_000,
        ) {
            let b = a + len;
            let offset = offset_raw % len;
            // `lead - offset` in `[1, len)`, so the lead fits one period.
            let deficit = 1 + deficit_raw % (len - 1);
            let lead = offset + deficit;
            let raw = a + offset;
            let published = rewind_in_loop(raw, lead, a, b);
            prop_assert_eq!(published, b - deficit);
        }
    }

    #[test]
    fn crossfade_gains_hard_cut_at_zero() {
        assert_eq!(crossfade_gains(0, 0), (1.0, 0.0));
    }

    #[test]
    fn effective_crossfade_zero_at_a_equals_zero() {
        assert_eq!(effective_crossfade(2_205, 0, 4_000), 0);
    }
}

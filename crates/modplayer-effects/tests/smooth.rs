// SPDX-License-Identifier: MIT OR Apache-2.0

//! contracts/engine-effect-chain.md §12: `Smoothed` — pins FR-010.

use modplayer_effects::smooth::Smoothed;

#[test]
fn ramp_reaches_target_in_exactly_ramp_frames() {
    let mut s = Smoothed::new(0.0);
    s.set_target(1.0, 10);
    for i in 0..9 {
        let v = s.advance();
        assert!(v < 1.0, "reached target early at frame {i} (v={v})");
        assert!(!s.is_settled());
    }
    let v = s.advance();
    assert!((v - 1.0).abs() < 1e-6, "v={v}");
    assert!(s.is_settled());
}

#[test]
fn retarget_restarts_from_current() {
    let mut s = Smoothed::new(0.0);
    s.set_target(1.0, 10);
    for _ in 0..5 {
        s.advance();
    }
    let mid = s.current;
    assert!(mid > 0.0 && mid < 1.0);
    s.set_target(0.0, 10);
    assert!(
        (s.current - mid).abs() < 1e-6,
        "retarget must start from current ({mid}), not jump to the old target or zero"
    );
    let v = s.advance();
    assert!(v < mid, "must now move toward the new target");
}

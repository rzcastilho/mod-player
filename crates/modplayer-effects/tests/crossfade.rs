// SPDX-License-Identifier: MIT OR Apache-2.0

//! contracts/engine-effect-chain.md §12: `Crossfade` — pins FR-005.

use modplayer_effects::crossfade::Crossfade;

#[test]
fn equal_power_and_exact_endpoints() {
    let mut cf = Crossfade::settled(false);
    assert_eq!(cf.gains(), (1.0, 0.0));

    cf.set_target(true, 5);
    assert!(!cf.is_settled());
    for _ in 0..5 {
        let (dry, wet) = cf.gains();
        let power = dry * dry + wet * wet;
        assert!((power - 1.0).abs() < 1e-4, "power={power}");
        cf.advance();
    }
    assert!(cf.is_settled());
    assert_eq!(cf.gains(), (0.0, 1.0), "must reach the exact endpoint");
}

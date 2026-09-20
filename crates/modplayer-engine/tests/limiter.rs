// SPDX-License-Identifier: MIT OR Apache-2.0

//! FR-025(e): ceiling and volume clamping is exhaustive and exact at the
//! type boundary — there is no way to construct an out-of-range value
//! (contracts/engine-commands.md).
//!
//! FR-025(d)/US2 acceptance 1: `limiter_never_exceeds_ceiling` sweeps every
//! ceiling in `-6.0..=-0.1` step 0.1 against the synthetic track's 0 dBFS
//! square segment, the loudest material the engine ever produces.

use modplayer_audio_source::AudioSource;
use modplayer_audio_source_synthetic::SyntheticSource;
use modplayer_engine::{CeilingDb, Limiter, VolumePercent};

#[test]
fn ceiling_and_cap_clamp() {
    assert_eq!(CeilingDb::new(0.0).db(), -0.1);
    assert_eq!(CeilingDb::new(-20.0).db(), -6.0);
    assert_eq!(VolumePercent::new(150).value(), 100);
}

#[test]
fn limiter_never_exceeds_ceiling_for_the_zero_dbfs_square_segment() {
    let rate = 44_100u32;
    let mut source = SyntheticSource::new(rate);
    // Segment 2 (10 s in): 1 kHz square @ 0 dBFS, full-scale ±1.0.
    source.seek(10 * u64::from(rate));
    let mut square_segment = vec![0.0f32; 512 * 2];
    source.fill(&mut square_segment);
    assert!(
        square_segment.iter().any(|&s| s.abs() > 0.99),
        "sanity: the square segment must actually reach full scale"
    );

    // Every ceiling in -6.0..=-0.1 step 0.1 (60 values), as integer tenths
    // to avoid float-step accumulation error.
    for tenths in -60..=-1 {
        let ceiling = CeilingDb::new(tenths as f32 / 10.0);
        let limiter = Limiter::new(ceiling);
        let mut buf = square_segment.clone();
        limiter.process(&mut buf);
        let max = buf.iter().fold(0.0f32, |m, &s| m.max(s.abs()));
        assert!(
            max <= ceiling.to_linear() + 1e-6,
            "ceiling {} dBFS exceeded: max |y| = {max}, ceiling_lin = {}",
            ceiling.db(),
            ceiling.to_linear()
        );
    }
}

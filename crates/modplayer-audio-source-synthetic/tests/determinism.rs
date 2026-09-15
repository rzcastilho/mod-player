// SPDX-License-Identifier: MIT OR Apache-2.0

//! Determinism test (contracts/audio-source.md): "Output is a pure
//! function of `(sample_rate, position)`; two instances at the same
//! position produce bit-identical frames." Two `SyntheticSource`s seeked to
//! the same frame produce identical buffers, and — crucially — a source
//! that arrives at a position incrementally (many small `fill` calls)
//! matches one that jumps there directly via `seek`, proving the output
//! never depends on how a caller got there.

use modplayer_audio_source::AudioSource;
use modplayer_audio_source_synthetic::SyntheticSource;

#[test]
fn seeking_to_the_same_frame_produces_identical_buffers() {
    let rate = 44_100u32;
    // One position per segment (sine, square, sawtooth sweep, silence),
    // plus the very start and a near-the-end frame.
    let seek_targets = [0u64, 1, 4_410, 220_500, 330_750, 617_400, 970_199];

    for &frame in &seek_targets {
        let mut a = SyntheticSource::new(rate);
        let mut b = SyntheticSource::new(rate);
        a.seek(frame);
        b.seek(frame);

        let mut out_a = vec![0.0f32; 512];
        let mut out_b = vec![0.0f32; 512];
        a.fill(&mut out_a);
        b.fill(&mut out_b);

        assert_eq!(out_a, out_b, "seek({frame}) must be bit-identical");
    }
}

#[test]
fn incremental_playback_matches_a_direct_seek_to_the_same_position() {
    let rate = 44_100u32;

    // Arrive at frame 5_000 by many small incremental `fill` calls.
    let mut incremental = SyntheticSource::new(rate);
    let mut scratch = vec![0.0f32; 200]; // 100 frames per chunk
    let mut advanced = 0u64;
    while advanced < 5_000 {
        incremental.fill(&mut scratch);
        advanced += 100;
    }
    assert_eq!(incremental.position(), advanced);

    // Arrive at the same frame directly via `seek`.
    let mut seeked = SyntheticSource::new(rate);
    seeked.seek(advanced);

    let mut out_incremental = vec![0.0f32; 64];
    let mut out_seeked = vec![0.0f32; 64];
    incremental.fill(&mut out_incremental);
    seeked.fill(&mut out_seeked);

    assert_eq!(
        out_incremental, out_seeked,
        "output at a position must not depend on how it was reached"
    );
}

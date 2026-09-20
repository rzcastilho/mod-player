// SPDX-License-Identifier: MIT OR Apache-2.0

//! 60 s continuity/dropout test (US2-5, backs FR-025): fills the synthetic
//! source in 256-frame chunks for 60 s and checks, for every frame produced
//! while the 440 Hz sine segment is sounding, that (1) no frame was dropped
//! or duplicated — the source's own frame index (independently tracked
//! here) always matches `position()` after each chunk — and (2) no
//! sample-to-sample jump exceeds the theoretical maximum for a 440 Hz sine
//! of the segment's amplitude at the render rate.

use std::f64::consts::PI;

use modplayer_audio_source::AudioSource;
use modplayer_audio_source_synthetic::SyntheticSource;

const SINE_FREQ_HZ: f64 = 440.0;
/// −12 dBFS peak, matching `track::PEAK_MINUS_12DB`.
const SINE_PEAK: f64 = 0.251_188_64;

#[test]
fn no_dropout_or_duplicate_within_the_sine_segment_over_60_seconds() {
    let rate = 44_100u32;
    let mut source = SyntheticSource::new(rate);
    let track_len = source.len_frames().unwrap_or(1).max(1);
    let seg1_len = 10 * u64::from(rate);

    let chunk_frames = 256usize;
    let total_frames = u64::from(rate) * 60;

    // The exact maximum possible difference between two adjacent samples
    // of a `SINE_PEAK`-amplitude sine at `SINE_FREQ_HZ`/`rate`:
    // `max|sin(a + d) - sin(a)| = 2 * sin(d / 2)` for step `d` (radians).
    let step = 2.0 * PI * SINE_FREQ_HZ / f64::from(rate);
    let max_jump = 2.0 * SINE_PEAK * (step / 2.0).sin() + 1e-4;

    // Independently tracked expected frame index, so a drop/duplicate bug
    // in `fill` would show up as a mismatch against `source.position()`.
    let mut expected_position = 0u64;
    let mut rendered = 0u64;
    let mut prev: Option<(u64, f32)> = None; // (position, sample)

    let mut buf = vec![0.0f32; chunk_frames * 2];
    while rendered < total_frames {
        let this_chunk = chunk_frames.min((total_frames - rendered) as usize);
        let slice = &mut buf[..this_chunk * 2];
        source.fill(slice);
        rendered += this_chunk as u64;

        let mut position = expected_position;
        for frame in 0..this_chunk {
            let sample = slice[frame * 2]; // L channel (mono content, both channels equal)

            if position < seg1_len {
                if let Some((prev_position, prev_sample)) = prev
                    && prev_position + 1 == position
                {
                    let jump = f64::from(sample - prev_sample).abs();
                    assert!(
                        jump <= max_jump,
                        "sample-to-sample jump {jump} exceeds bound {max_jump} at position {position}"
                    );
                }
                prev = Some((position, sample));
            } else {
                prev = None;
            }

            position = (position + 1) % track_len;
        }

        expected_position = position;
        assert_eq!(
            source.position(),
            expected_position,
            "position must advance by exactly the frames rendered, no drop/duplicate"
        );
    }
}

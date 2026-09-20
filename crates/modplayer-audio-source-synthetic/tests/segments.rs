// SPDX-License-Identifier: MIT OR Apache-2.0

//! Segment peak levels test (contracts/audio-source.md): a full 22 s fill
//! at 44 100 Hz produces the four track segments' peaks in order: −12 dBFS
//! sine (0.2512), 0 dBFS square (1.0), −12 dBFS sawtooth sweep (0.2512),
//! silence (0.0).

use modplayer_audio_source::AudioSource;
use modplayer_audio_source_synthetic::SyntheticSource;

#[test]
fn segment_peaks_match_contract() {
    let rate = 44_100u32;
    let mut source = SyntheticSource::new(rate);
    let track_len = source.len_frames().unwrap_or_default() as usize;
    let mut buf = vec![0.0f32; track_len * 2];
    source.fill(&mut buf);

    // Segment boundaries in frames: 0, 10s, 15s, 20s, 22s.
    let bounds = [
        0usize,
        10 * rate as usize,
        15 * rate as usize,
        20 * rate as usize,
        track_len,
    ];
    let expected_peaks = [0.2512_f32, 1.0, 0.2512, 0.0];
    let names = [
        "sine @ -12 dBFS",
        "square @ 0 dBFS",
        "sawtooth sweep @ -12 dBFS",
        "silence",
    ];

    for i in 0..4 {
        let start = bounds[i] * 2;
        let end = bounds[i + 1] * 2;
        let peak = buf[start..end]
            .iter()
            .fold(0.0f32, |max, &s| max.max(s.abs()));
        assert!(
            (peak - expected_peaks[i]).abs() < 1e-4,
            "segment {} ({}) peak was {peak}, expected {}",
            i + 1,
            names[i],
            expected_peaks[i]
        );
    }
}

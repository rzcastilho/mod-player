// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! `DecodedStore` tests pinning contracts/decoded-store.md §5
//! (005-now-playing-waveform, T007/T008).

use modplayer_audio_source::{
    CHUNK_FRAMES, DecodedStore, MAX_STORE_FRAMES, PeakBucket, StoreState,
};
use proptest::prelude::*;

/// `round(clamp(sample, -1, 1) * 127)` (contracts/decoded-store.md §2, rule
/// 6) — duplicated from the documented formula so the fold test verifies
/// against the contract, not against the implementation's own helper.
fn expected_quantise(x: f32) -> i8 {
    (x.clamp(-1.0, 1.0) * 127.0).round() as i8
}

/// Deterministic, bounded `[-1, 1)` content — a pure function of `frame`,
/// distinct per L/R channel.
fn tone_at(frame: u64) -> (f32, f32) {
    let l = ((frame % 2000) as f32 / 2000.0) * 2.0 - 1.0;
    let r = (((frame + 977) % 2000) as f32 / 2000.0) * 2.0 - 1.0;
    (l, r)
}

fn make_buf(from_frame: u64, count: u64) -> Vec<f32> {
    let mut buf = Vec::with_capacity((count * 2) as usize);
    for i in 0..count {
        let (l, r) = tone_at(from_frame + i);
        buf.push(l);
        buf.push(r);
    }
    buf
}

#[test]
fn store_write_then_covers_and_reads_back_exact() {
    let watermark = 3 * CHUNK_FRAMES + 100;
    let store = DecodedStore::new(44_100, watermark + 5_000);

    let buf = make_buf(0, watermark);
    let written = store.write_frames(0, &buf);
    assert_eq!(written, watermark as usize);

    // Covered up to (not including) the watermark.
    assert!(store.covers(0));
    assert!(store.covers(watermark - 1));
    assert!(!store.covers(watermark));
    assert!(!store.covers(watermark + 10));
    assert_eq!(store.covered_frames(), watermark);

    // Exact bit-for-bit read back of every covered frame.
    let mut out = vec![0.0f32; (watermark * 2) as usize];
    let read = store.read_frames(0, &mut out);
    assert_eq!(read, watermark as usize);
    assert_eq!(out, buf);

    // Reading past the watermark stops exactly there.
    let mut out2 = vec![-1.0f32; ((watermark + 50) * 2) as usize];
    let read2 = store.read_frames(0, &mut out2);
    assert_eq!(read2, watermark as usize);
}

#[test]
fn store_rejects_non_aligned_writes() {
    let store = DecodedStore::new(44_100, CHUNK_FRAMES * 2);

    // A write starting mid-chunk into an *empty* chunk (offset 5 != the
    // chunk's filled watermark of 0) is rejected wholesale.
    let buf = make_buf(5, 10);
    let written = store.write_frames(5, &buf);
    assert_eq!(written, 0);
    assert_eq!(store.covered_frames(), 0);
    assert!(!store.covers(5));
    assert!(!store.covers(0));
}

#[test]
fn store_fold_peaks_mono_min_max_quantised() {
    let store = DecodedStore::new(44_100, 8);
    // Bucket 1 (frames 0..4): extremes on both channels, at different
    // frames, to pin the "min/max across both channels" fold rule.
    let bucket1 = vec![
        1.0, -1.0, // frame 0
        -0.5, 0.25, // frame 1
        0.0, 0.0, // frame 2
        0.6, -1.0, // frame 3
    ];
    let written = store.write_frames(0, &bucket1);
    assert_eq!(written, 4);

    let mut out = vec![PeakBucket::default(); 2];
    // Bucket 2 (frames 4..8) is not written yet — fold stops after the
    // first (complete) bucket.
    let count = store.fold_peaks(0, 4, &mut out);
    assert_eq!(count, 1);
    assert_eq!(
        out[0],
        PeakBucket {
            min: expected_quantise(-1.0),
            max: expected_quantise(1.0),
        }
    );

    // Completing bucket 2 with a known, distinct extreme lets `fold_peaks`
    // advance a second bucket from a non-zero `from_frame` watermark.
    let bucket2 = vec![
        0.2, 0.2, // frame 4
        0.2, 0.2, // frame 5
        0.9, -0.3, // frame 6
        0.1, 0.1, // frame 7
    ];
    let written2 = store.write_frames(4, &bucket2);
    assert_eq!(written2, 4);
    let mut out2 = vec![PeakBucket::default(); 1];
    let count2 = store.fold_peaks(4, 4, &mut out2);
    assert_eq!(count2, 1);
    assert_eq!(
        out2[0],
        PeakBucket {
            min: expected_quantise(-0.3),
            max: expected_quantise(0.9),
        }
    );
}

#[test]
fn store_cap_never_allocates_past_bound() {
    // A ~20-minute track at 44.1 kHz well exceeds MAX_STORE_FRAMES.
    let store = DecodedStore::new(44_100, MAX_STORE_FRAMES + CHUNK_FRAMES * 5);

    // A legitimately aligned append (the last in-cap chunk's own start)
    // that asks for more than the cap has room for is truncated exactly
    // at the boundary — the slot table never grows past it.
    let last_chunk_start = MAX_STORE_FRAMES - CHUNK_FRAMES;
    let buf = make_buf(last_chunk_start, CHUNK_FRAMES + 20);
    let written = store.write_frames(last_chunk_start, &buf);
    assert_eq!(written, CHUNK_FRAMES as usize);
    assert!(store.covers(MAX_STORE_FRAMES - 1));
    assert!(!store.covers(MAX_STORE_FRAMES));

    // A track longer than the cap can never reach `Complete` even once
    // every in-cap slot is filled — `set_complete` with an
    // `exact_len_frames` beyond capacity is a no-op.
    store.set_complete(MAX_STORE_FRAMES + CHUNK_FRAMES * 5);
    assert_eq!(store.state(), StoreState::Filling);
}

#[test]
fn store_terminal_states_reject_writes() {
    // `Complete`: exactly filled, then terminal.
    let store = DecodedStore::new(44_100, 10);
    let buf = make_buf(0, 10);
    assert_eq!(store.write_frames(0, &buf), 10);
    store.set_complete(10);
    assert_eq!(store.state(), StoreState::Complete);
    assert_eq!(store.write_frames(0, &make_buf(0, 1)), 0);
    assert_eq!(store.covered_frames(), 10);

    // `Failed`: terminal immediately, no further writes accepted.
    let failed = DecodedStore::new(44_100, 10);
    failed.set_failed();
    assert_eq!(failed.state(), StoreState::Failed);
    assert_eq!(failed.write_frames(0, &make_buf(0, 1)), 0);
    assert_eq!(failed.covered_frames(), 0);

    // `set_complete`/`set_failed` after a terminal state are no-ops.
    failed.set_complete(10);
    assert_eq!(failed.state(), StoreState::Failed);
}

#[test]
fn store_debug_is_redacted() {
    let store = DecodedStore::new(44_100, 100);
    let telltale = 0.123_456_f32;
    let mut buf = make_buf(0, 4);
    buf[0] = telltale;
    store.write_frames(0, &buf);

    let debug = format!("{store:?}");
    assert!(debug.contains("DecodedStore"));
    assert!(debug.contains("sample_rate"));
    assert!(debug.contains("covered_frames"));
    assert!(!debug.contains(&telltale.to_string()));
}

proptest! {
    #[test]
    fn store_read_back_is_identity_for_any_aligned_write_sequence(
        pieces in prop::collection::vec(1usize..50, 0..40),
    ) {
        let total: u64 = pieces.iter().map(|&n| n as u64).sum();
        let store = DecodedStore::new(44_100, total + 1);

        let mut written_total = 0u64;
        for &n in &pieces {
            let buf = make_buf(written_total, n as u64);
            let written = store.write_frames(written_total, &buf);
            prop_assert_eq!(written, n);
            written_total += n as u64;
        }

        let expected = make_buf(0, total);
        let mut out = vec![0.0f32; (total * 2) as usize];
        let read = store.read_frames(0, &mut out);
        prop_assert_eq!(read as u64, total);
        prop_assert_eq!(out, expected);
    }
}

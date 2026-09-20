// SPDX-License-Identifier: MIT OR Apache-2.0

//! T040: `ConnectRtSource::fill`/`seek` must never allocate
//! (contracts/connect-source.md §7, contracts/audio-source-host.md §4).
//! 005-now-playing-waveform (T016) extends this with a store-attached
//! case (contracts/connect-source-delta.md §3 guarantee #8): `fill`/
//! `seek` must allocate nothing on either feed.

use std::sync::Arc;

use assert_no_alloc::{AllocDisabler, assert_no_alloc};
use modplayer_audio_source::{AudioSource, DecodedStore, SourceRtShared};
use rtrb::RingBuffer;

#[cfg(debug_assertions)]
#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

/// `ConnectRtSource::new` is crate-private; a small `#[path]` include
/// keeps this integration test using the exact same real-time code
/// without re-exporting internals from the crate's public API.
#[path = "../src/program.rs"]
#[allow(dead_code)]
mod program;
#[path = "../src/rt.rs"]
#[allow(dead_code)]
mod rt;

use program::Marker;
use rt::ConnectRtSource;

const RETIRED_CAPACITY: usize = 65;

#[test]
fn fill_and_seek_never_allocate() {
    let (mut sample_tx, sample_rx) = RingBuffer::<f32>::new(16_384);
    let (mut marker_tx, marker_rx) = RingBuffer::<Marker>::new(64);
    let (retired_tx, _retired_rx) = RingBuffer::<Arc<DecodedStore>>::new(RETIRED_CAPACITY);
    let shared = Arc::new(SourceRtShared::new());
    shared.set_track_len_frames(44_100 * 10);
    let store = DecodedStore::new(44_100, 44_100 * 10);
    let _ = marker_tx.push(Marker::track_start(0, store));
    for _ in 0..4096 {
        let _ = sample_tx.push(0.1);
        let _ = sample_tx.push(-0.1);
    }

    let mut source = ConnectRtSource::new(sample_rx, marker_rx, shared, 0, retired_tx);
    let mut out = [0.0f32; 2048];

    assert_no_alloc(|| {
        source.fill(&mut out);
        source.fill(&mut out);
        source.seek(1_000);
        source.fill(&mut out);
    });
}

/// Same shape, but the store is already fully decoded and covers the
/// whole seek target — exercising the `Store` feed path, `read_frames`,
/// and a `TrackStart` retirement, all inside `assert_no_alloc`.
#[test]
fn fill_and_seek_never_allocate_with_a_store_attached() {
    let (mut sample_tx, sample_rx) = RingBuffer::<f32>::new(16_384);
    let (mut marker_tx, marker_rx) = RingBuffer::<Marker>::new(64);
    let (retired_tx, _retired_rx) = RingBuffer::<Arc<DecodedStore>>::new(RETIRED_CAPACITY);
    let shared = Arc::new(SourceRtShared::new());
    let len_frames = 44_100 * 10;
    shared.set_track_len_frames(len_frames);

    let first_store = DecodedStore::new(44_100, len_frames);
    let interleaved: Vec<f32> = (0..len_frames).flat_map(|_| [0.2, -0.2]).collect();
    first_store.write_frames(0, &interleaved);
    first_store.set_complete(len_frames);
    let _ = marker_tx.push(Marker::track_start(0, first_store));

    let second_store = DecodedStore::new(44_100, len_frames);
    second_store.write_frames(0, &interleaved);
    second_store.set_complete(len_frames);
    // Applied inside the `assert_no_alloc` block below, at a later
    // `at_written_frame` (reached by the third/fourth `fill`, each
    // consuming 1_024 frames) so it lands mid-run and exercises the
    // retirement-ring push (never a drop) alongside `fill`.
    let _ = marker_tx.push(Marker::track_start(3_000, second_store));

    for _ in 0..4096 {
        let _ = sample_tx.push(0.1);
        let _ = sample_tx.push(-0.1);
    }

    let mut source = ConnectRtSource::new(sample_rx, marker_rx, shared, 0, retired_tx);
    let mut out = [0.0f32; 2048];

    assert_no_alloc(|| {
        source.fill(&mut out); // applies the first TrackStart, feeds from the ring
        source.seek(5_000); // covered by the store: switches to the Store feed
        source.fill(&mut out);
        source.fill(&mut out); // consumed_frames now passes 3_000: applies the second TrackStart
    });
}

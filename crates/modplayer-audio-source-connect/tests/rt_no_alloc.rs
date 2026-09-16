// SPDX-License-Identifier: MIT OR Apache-2.0

//! T040: `ConnectRtSource::fill`/`seek` must never allocate
//! (contracts/connect-source.md §7, contracts/audio-source-host.md §4).

use std::sync::Arc;

use assert_no_alloc::{AllocDisabler, assert_no_alloc};
use modplayer_audio_source::{AudioSource, SourceRtShared};
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

#[test]
fn fill_and_seek_never_allocate() {
    let (mut sample_tx, sample_rx) = RingBuffer::<f32>::new(16_384);
    let (mut marker_tx, marker_rx) = RingBuffer::<Marker>::new(64);
    let shared = Arc::new(SourceRtShared::new());
    shared.set_track_len_frames(44_100 * 10);
    let _ = marker_tx.push(Marker::track_start(0));
    for _ in 0..4096 {
        let _ = sample_tx.push(0.1);
        let _ = sample_tx.push(-0.1);
    }

    let mut source = ConnectRtSource::new(sample_rx, marker_rx, shared, 0);
    let mut out = [0.0f32; 2048];

    assert_no_alloc(|| {
        source.fill(&mut out);
        source.fill(&mut out);
        source.seek(1_000);
        source.fill(&mut out);
    });
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! T041: marker sequencing — `TrackStart` after `TrackEnd` at the same
//! `at_written_frame` advances `track_seq` exactly once
//! (contracts/audio-source-host.md §5, contracts/connect-source.md §7).

use std::sync::Arc;

use modplayer_audio_source::{AudioSource, SourceRtShared};
use rtrb::RingBuffer;

#[path = "../src/program.rs"]
#[allow(dead_code)]
mod program;
#[path = "../src/rt.rs"]
#[allow(dead_code)]
mod rt;

use program::Marker;
use rt::ConnectRtSource;

#[test]
fn track_end_then_track_start_at_the_same_boundary_advances_seq_exactly_once() {
    let (mut sample_tx, sample_rx) = RingBuffer::<f32>::new(64);
    let (mut marker_tx, marker_rx) = RingBuffer::<Marker>::new(8);
    let shared = Arc::new(SourceRtShared::new());
    let starting_seq = shared.track_seq();

    // Both markers land at the very first frame boundary (0), pushed in
    // the order the worker would emit them: the old track's `TrackEnd`
    // strictly before the new track's `TrackStart` (contract §3).
    let _ = marker_tx.push(Marker::track_end(0));
    let _ = marker_tx.push(Marker::track_start(0));
    for _ in 0..4 {
        let _ = sample_tx.push(0.0);
        let _ = sample_tx.push(0.0);
    }

    let mut source = ConnectRtSource::new(sample_rx, marker_rx, Arc::clone(&shared), 0);
    let mut out = [0.0f32; 4]; // 2 frames
    source.fill(&mut out);

    assert_eq!(
        shared.track_seq(),
        starting_seq.wrapping_add(1),
        "track_seq must advance exactly once, not twice, for the TrackEnd+TrackStart pair"
    );
    assert_eq!(
        source.position(),
        2,
        "position resets to 0 at TrackStart, then advances by delivered frames"
    );
}

#[test]
fn markers_due_at_a_later_boundary_do_not_apply_early() {
    let (mut sample_tx, sample_rx) = RingBuffer::<f32>::new(64);
    let (mut marker_tx, marker_rx) = RingBuffer::<Marker>::new(8);
    let shared = Arc::new(SourceRtShared::new());
    let starting_seq = shared.track_seq();

    // Due only once 100 frames have been consumed — far beyond this
    // buffer's 2 frames, so it must not apply yet.
    let _ = marker_tx.push(Marker::track_start(100));
    for _ in 0..4 {
        let _ = sample_tx.push(0.0);
        let _ = sample_tx.push(0.0);
    }
    let mut source = ConnectRtSource::new(sample_rx, marker_rx, shared.clone(), 500);
    let mut out = [0.0f32; 4];
    source.fill(&mut out);

    assert_eq!(
        shared.track_seq(),
        starting_seq,
        "not-yet-due marker must not apply"
    );
    assert_eq!(source.position(), 502);
}

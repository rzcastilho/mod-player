// SPDX-License-Identifier: MIT OR Apache-2.0

//! `ConnectRtSource`'s store/ring feed rules (005-now-playing-waveform,
//! contracts/connect-source-delta.md §3, data-model.md §2.2). `#[path]`-
//! includes `program.rs`/`rt.rs` directly (like `tests/markers.rs`) so
//! these tests can drive the real-time half with hand-built rings and no
//! network, and — since a couple of `ConnectRtSource` fields are
//! `pub(crate)` for exactly this purpose — set up a store directly rather
//! than only through a `TrackStart` marker.

use std::sync::Arc;

use modplayer_audio_source::{AudioSource, DecodedStore, SourceRtShared};
use rtrb::RingBuffer;

#[path = "../src/program.rs"]
#[allow(dead_code)]
mod program;
#[path = "../src/rt.rs"]
#[allow(dead_code)]
mod rt;

use program::Marker;
use rt::ConnectRtSource;

/// Mirrors `lib.rs`'s `MARKER_CAPACITY + 1`.
const RETIRED_CAPACITY: usize = 65;
const SAMPLE_RATE: u32 = 44_100;

#[allow(clippy::type_complexity)]
fn build(
    position: u64,
) -> (
    rtrb::Producer<f32>,
    rtrb::Producer<Marker>,
    rtrb::Consumer<Arc<DecodedStore>>,
    ConnectRtSource,
) {
    let (sample_tx, sample_rx) = RingBuffer::<f32>::new(64);
    let (marker_tx, marker_rx) = RingBuffer::<Marker>::new(8);
    let (retired_tx, retired_rx) = RingBuffer::<Arc<DecodedStore>>::new(RETIRED_CAPACITY);
    let shared = Arc::new(SourceRtShared::new());
    let source = ConnectRtSource::new(sample_rx, marker_rx, shared, position, retired_tx);
    (sample_tx, marker_tx, retired_rx, source)
}

/// A `Complete` store whose sample at frame `i` is `(0.1 + i, -0.1 - i)`
/// — distinct per frame, so a test can assert exactly which frame `fill`
/// read.
fn filled_store(frames: u64) -> Arc<DecodedStore> {
    let store = DecodedStore::new(SAMPLE_RATE, frames);
    let interleaved: Vec<f32> = (0..frames)
        .flat_map(|i| [0.1 + i as f32, -0.1 - i as f32])
        .collect();
    store.write_frames(0, &interleaved);
    store.set_complete(frames);
    store
}

#[test]
fn rt_seek_into_store_is_sample_exact() {
    let (_sample_tx, _marker_tx, _retired_rx, mut source) = build(0);
    let store = filled_store(10);
    source.store = Some(Arc::clone(&store));

    source.seek(3);
    let mut out = [0.0f32; 4]; // 2 frames
    source.fill(&mut out);

    assert_eq!(out[0], 0.1 + 3.0);
    assert_eq!(out[1], -0.1 - 3.0);
    assert_eq!(out[2], 0.1 + 4.0);
    assert_eq!(out[3], -0.1 - 4.0);
    assert_eq!(source.position(), 5);
}

#[test]
fn rt_seek_outside_store_uses_ring() {
    let (mut sample_tx, _marker_tx, _retired_rx, mut source) = build(0);
    let store = filled_store(4); // only covers frames 0..4
    source.store = Some(store);
    source.seek(100); // outside the store's coverage

    for _ in 0..2 {
        let _ = sample_tx.push(0.3);
        let _ = sample_tx.push(-0.3);
    }
    let mut out = [0.0f32; 4];
    source.fill(&mut out);
    assert_eq!(out, [0.3, -0.3, 0.3, -0.3]);
    assert_eq!(source.position(), 102);
}

#[test]
fn rt_store_feed_drains_ring_in_lockstep() {
    let (mut sample_tx, _marker_tx, _retired_rx, mut source) = build(0);
    let store = filled_store(10);
    source.store = Some(store);
    source.seek(0);
    for _ in 0..5 {
        let _ = sample_tx.push(9.0);
        let _ = sample_tx.push(9.0);
    }
    assert_eq!(sample_tx.slots(), 64 - 10);

    let mut out = [0.0f32; 6]; // 3 frames from the store
    source.fill(&mut out);
    // The ring must have been drained by exactly 3 frames too.
    assert_eq!(sample_tx.slots(), 64 - 10 + 6);
}

#[test]
fn rt_reposition_marker_ignored_on_store_feed() {
    let (_sample_tx, mut marker_tx, _retired_rx, mut source) = build(0);
    let store = filled_store(10);
    source.store = Some(store);
    source.seek(2);
    let _ = marker_tx.push(Marker::reposition(0, 9_999));
    let mut out = [0.0f32; 2];
    source.fill(&mut out);
    // `cursor` must not have jumped to 9_999 — it only advanced by the
    // one frame delivered from the store.
    assert_eq!(source.position(), 3);
}

#[test]
fn rt_track_start_swaps_store_retires_old() {
    let (_sample_tx, mut marker_tx, mut retired_rx, mut source) = build(0);
    let old_store = filled_store(4);
    source.store = Some(Arc::clone(&old_store));

    let new_store = filled_store(4);
    let _ = marker_tx.push(Marker::track_start(0, Arc::clone(&new_store)));
    let mut out = [0.0f32; 2];
    source.fill(&mut out);

    assert!(
        source
            .store
            .as_ref()
            .is_some_and(|s| Arc::ptr_eq(s, &new_store))
    );
    let retired = retired_rx.pop().unwrap_or_else(|_| unreachable!());
    assert!(Arc::ptr_eq(&retired, &old_store));
}

#[test]
fn store_drop_never_on_rt() {
    let (_sample_tx, mut marker_tx, mut retired_rx, mut source) = build(0);
    let a = filled_store(4);
    let b = filled_store(4);
    let c = filled_store(4);
    let weak_a = Arc::downgrade(&a);
    let weak_b = Arc::downgrade(&b);
    let weak_c = Arc::downgrade(&c);

    // Each `Arc` is moved straight into its `TrackStart` marker — from
    // here on the *only* non-test strong references are the RT's
    // `store` field and the retirement ring, never a local binding.
    let _ = marker_tx.push(Marker::track_start(0, a));
    let mut out = [0.0f32; 2];
    source.fill(&mut out); // applies A's TrackStart; no previous store to retire
    assert!(weak_a.upgrade().is_some(), "A is alive via source.store");

    let _ = marker_tx.push(Marker::track_start(0, b));
    source.fill(&mut out); // applies B's TrackStart, retires A
    assert!(
        weak_a.upgrade().is_some(),
        "A must still be alive until the worker drains the retirement ring"
    );

    let _ = marker_tx.push(Marker::track_start(0, c));
    source.fill(&mut out); // applies C's TrackStart, retires B
    assert!(
        weak_b.upgrade().is_some(),
        "B must still be alive until the worker drains the retirement ring"
    );

    // Drain the ring (the worker's job) and confirm every retirement
    // arrived, in order, and only then would they be free to drop.
    let retired_a = retired_rx.pop().unwrap_or_else(|_| unreachable!());
    assert!(
        weak_a.upgrade().is_some(),
        "still alive: this local binding is itself a strong ref"
    );
    drop(retired_a);
    assert!(
        weak_a.upgrade().is_none(),
        "A drops only once the worker's last reference is gone"
    );
    let retired_b = retired_rx.pop().unwrap_or_else(|_| unreachable!());
    assert!(weak_b.upgrade().is_some());
    drop(retired_b);
    assert!(
        weak_c.upgrade().is_some(),
        "C is still current on the RT, never retired"
    );
}

#[test]
fn rt_retire_never_full() {
    let (_sample_tx, mut marker_tx, _retired_rx, mut source) = build(0);
    // `MARKER_CAPACITY + 1` TrackStart markers, applied without any drain
    // of the retirement ring — the ring's capacity (also `MARKER_CAPACITY
    // + 1`) must never overflow into `parked`.
    source.store = Some(filled_store(2));
    for i in 0..65u64 {
        let _ = marker_tx.push(Marker::track_start(i, filled_store(2)));
    }
    let mut out = [0.0f32; 2];
    for _ in 0..65 {
        source.fill(&mut out);
    }
    assert!(
        source.parked.is_none(),
        "retirement ring must never fill up"
    );
}

#[test]
fn rt_store_rescues_ring_underrun() {
    let (_sample_tx, _marker_tx, _retired_rx, mut source) = build(0);
    // Ring is empty; the store covers the current cursor (0).
    let store = filled_store(10);
    source.store = Some(store);
    let mut out = [0.0f32; 2];
    source.fill(&mut out);
    assert_eq!(out[0], 0.1);
    assert_eq!(out[1], -0.1);
    assert_eq!(source.position(), 1);
}

#[test]
fn rt_store_exhaustion_falls_back_to_ring() {
    let (mut sample_tx, mut marker_tx, _retired_rx, mut source) = build(0);
    let store = filled_store(2); // covers only frames 0..2
    source.store = Some(store);
    source.seek(0);
    let _ = marker_tx.push(Marker::reposition(0, 500));
    for _ in 0..3 {
        let _ = sample_tx.push(7.0);
        let _ = sample_tx.push(7.0);
    }
    let mut out = [0.0f32; 6]; // 3 frames: 2 from the store, then exhaustion
    source.fill(&mut out);
    assert_eq!(out[0], 0.1);
    assert_eq!(out[2], 0.1 + 1.0);
    // Falls back to the ring at its own marker-derived baseline (500),
    // then delivers one ring frame, advancing to 501.
    assert_eq!(source.position(), 501);
}

#[test]
fn rt_position_tracks_store_cursor() {
    let (_sample_tx, _marker_tx, _retired_rx, mut source) = build(0);
    let store = filled_store(20);
    source.store = Some(store);
    source.seek(5);
    for _ in 0..4 {
        let mut out = [0.0f32; 4]; // 2 frames per fill
        source.fill(&mut out);
    }
    assert_eq!(source.position(), 5 + 8);
}

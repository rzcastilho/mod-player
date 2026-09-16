// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! T042: `RingSink` back-pressure — writing N samples through a ring of
//! capacity < N loses none, with a fake consumer thread draining
//! concurrently (contracts/connect-source.md §7).

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;

use librespot_playback::audio_backend::Sink;
use librespot_playback::convert::Converter;
use librespot_playback::decoder::AudioPacket;
use rtrb::RingBuffer;

#[path = "../src/sink.rs"]
#[allow(dead_code)]
mod sink;

use sink::RingSink;

#[test]
fn back_pressure_delivers_every_sample_through_a_smaller_ring() {
    const RING_CAPACITY: usize = 16;
    const SAMPLE_COUNT: usize = 5_000;

    let (producer, mut consumer) = RingBuffer::<f32>::new(RING_CAPACITY);
    let written = Arc::new(AtomicU64::new(0));
    let mut ring_sink = RingSink::new(producer, Arc::clone(&written));

    let samples: Vec<f64> = (0..SAMPLE_COUNT)
        .map(|i| (i as f64 % 100.0) / 100.0)
        .collect();
    let expected = samples.len();

    let consumer_thread = thread::spawn(move || {
        let mut received = Vec::with_capacity(expected);
        while received.len() < expected {
            match consumer.pop() {
                Ok(sample) => received.push(sample),
                Err(_) => thread::yield_now(),
            }
        }
        received
    });

    let none: Option<librespot_playback::config::DithererBuilder> = None;
    let mut converter = Converter::new(none);
    ring_sink
        .write(AudioPacket::Samples(samples.clone()), &mut converter)
        .expect("RingSink::write must never fail on a valid packet");

    let received = consumer_thread.join().expect("consumer thread panicked");
    assert_eq!(
        received.len(),
        samples.len(),
        "no sample may be lost under back-pressure"
    );
    for (expected, actual) in samples.iter().zip(received.iter()) {
        let converted = *expected as f32;
        assert!(
            (converted - actual).abs() < f32::EPSILON,
            "sample order/value must be preserved"
        );
    }
    assert_eq!(written.load(Ordering::Acquire), (samples.len() / 2) as u64);
}

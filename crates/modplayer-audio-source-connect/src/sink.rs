// SPDX-License-Identifier: MIT OR Apache-2.0

//! `RingSink`: librespot's `Sink`, writing decoded stereo `f32` frames into
//! an `rtrb::Producer<f32>` for `ConnectRtSource::fill` to pop
//! (contracts/connect-source.md §1, research R4). Runs on librespot's own
//! `Player` thread, not the audio callback — blocking briefly under
//! back-pressure is fine here (a bounded park, never a lock).

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

use librespot_playback::audio_backend::{Sink, SinkError, SinkResult};
use librespot_playback::convert::Converter;
use librespot_playback::decoder::AudioPacket;
use rtrb::Producer;

/// How long to park between retries when the ring is full (contract §1).
const BACKPRESSURE_PARK: Duration = Duration::from_micros(500);

/// Writes decoded interleaved-stereo `f32` samples into a lock-free ring,
/// parking briefly under back-pressure rather than dropping samples
/// (contract §1: "a ring of capacity < N lost none").
pub struct RingSink {
    producer: Producer<f32>,
    /// Total stereo frames written since construction — the write-side
    /// clock markers (`program.rs::Marker`) are stamped against
    /// (research R4).
    written_frames: Arc<AtomicU64>,
}

impl RingSink {
    pub fn new(producer: Producer<f32>, written_frames: Arc<AtomicU64>) -> Self {
        Self {
            producer,
            written_frames,
        }
    }

    fn push_sample(&mut self, sample: f32) {
        let mut value = sample;
        loop {
            match self.producer.push(value) {
                Ok(()) => return,
                Err(rtrb::PushError::Full(rejected)) => {
                    value = rejected;
                    thread::park_timeout(BACKPRESSURE_PARK);
                }
            }
        }
    }
}

impl Sink for RingSink {
    fn write(&mut self, packet: AudioPacket, converter: &mut Converter) -> SinkResult<()> {
        let samples = match packet {
            AudioPacket::Samples(samples) => samples,
            AudioPacket::Raw(_) => {
                return Err(SinkError::InvalidParams(
                    "RingSink expects decoded samples, not raw bytes".to_string(),
                ));
            }
        };
        let samples_f32 = converter.f64_to_f32(&samples);
        for sample in &samples_f32 {
            self.push_sample(*sample);
        }
        // Interleaved stereo: two `f32` per frame.
        self.written_frames
            .fetch_add((samples_f32.len() / 2) as u64, Ordering::Release);
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;
    use librespot_playback::config::DithererBuilder;
    use rtrb::RingBuffer;
    use std::thread;

    fn converter() -> Converter {
        let none: Option<DithererBuilder> = None;
        Converter::new(none)
    }

    #[test]
    fn back_pressure_never_loses_samples() {
        // Ring smaller than the packet: a consumer thread drains
        // concurrently so the producer's park loop makes progress.
        let (producer, mut consumer) = RingBuffer::<f32>::new(8);
        let written = Arc::new(AtomicU64::new(0));
        let mut sink = RingSink::new(producer, Arc::clone(&written));

        let samples: Vec<f64> = (0..200).map(|i| f64::from(i) / 200.0).collect();
        let expected_len = samples.len();

        let consumed = thread::spawn(move || {
            let mut received = Vec::with_capacity(expected_len);
            while received.len() < expected_len {
                match consumer.pop() {
                    Ok(v) => received.push(v),
                    Err(_) => thread::yield_now(),
                }
            }
            received
        });

        sink.write(AudioPacket::Samples(samples.clone()), &mut converter())
            .expect("write must succeed");

        let received = consumed.join().expect("consumer thread must not panic");
        assert_eq!(received.len(), samples.len());
        assert_eq!(written.load(Ordering::Acquire), (samples.len() / 2) as u64);
    }

    #[test]
    fn raw_packets_are_rejected() {
        let (producer, _consumer) = RingBuffer::<f32>::new(8);
        let mut sink = RingSink::new(producer, Arc::new(AtomicU64::new(0)));
        let result = sink.write(AudioPacket::Raw(vec![0u8; 4]), &mut converter());
        assert!(result.is_err());
    }
}

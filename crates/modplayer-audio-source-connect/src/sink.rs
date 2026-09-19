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
use modplayer_audio_source::SourceRtShared;
use rtrb::Producer;

use crate::swap::RingSwap;

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
    /// Where `attach()` parks the producer of a re-attached RT's ring
    /// (`swap.rs`); checked once per `write`.
    swap: Arc<RingSwap>,
    /// The RT-side consumed-frames clock, credited with whatever the
    /// abandoned ring still held at a swap so markers stamped against
    /// `written_frames` stay due at the right moment on the new ring.
    shared: Arc<SourceRtShared>,
}

impl RingSink {
    pub fn new(
        producer: Producer<f32>,
        written_frames: Arc<AtomicU64>,
        swap: Arc<RingSwap>,
        shared: Arc<SourceRtShared>,
    ) -> Self {
        Self {
            producer,
            written_frames,
            swap,
            shared,
        }
    }

    /// Adopt a re-attached RT's ring if one is parked. The old ring's
    /// unread samples are gone with its consumer, so their frame count is
    /// added to the consumed clock: every marker stamped at a write-side
    /// frame beyond them must still fall due on the new ring.
    fn adopt_parked_producer(&mut self) {
        if let Some(fresh) = self.swap.take_sample() {
            let capacity = self.producer.buffer().capacity();
            let unread_samples = capacity.saturating_sub(self.producer.slots());
            self.producer = fresh;
            self.shared.add_consumed_frames((unread_samples / 2) as u64);
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
        self.adopt_parked_producer();
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
    use crate::program::Marker;
    use librespot_playback::config::DithererBuilder;
    use modplayer_audio_source::DecodedStore;
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
        let mut sink = RingSink::new(
            producer,
            Arc::clone(&written),
            Arc::new(RingSwap::default()),
            Arc::new(SourceRtShared::new()),
        );

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

    /// `swap.rs`: a producer parked for a re-attached RT is adopted on
    /// the next `write`; the samples land in the new ring, and the old
    /// ring's unread frames are credited to the consumed clock so
    /// markers stamped against `written_frames` stay on time.
    #[test]
    fn parked_producer_is_adopted_and_unread_frames_credited() {
        let (old_producer, old_consumer) = RingBuffer::<f32>::new(64);
        let written = Arc::new(AtomicU64::new(0));
        let swap = Arc::new(RingSwap::default());
        let shared = Arc::new(SourceRtShared::new());
        let mut sink = RingSink::new(
            old_producer,
            Arc::clone(&written),
            Arc::clone(&swap),
            Arc::clone(&shared),
        );

        // 10 frames into the old ring, none of them consumed.
        sink.write(AudioPacket::Samples(vec![0.5; 20]), &mut converter())
            .unwrap();
        assert_eq!(shared.consumed_frames(), 0);

        let (new_producer, mut new_consumer) = RingBuffer::<f32>::new(64);
        let (marker_tx, _marker_rx) = RingBuffer::<Marker>::new(4);
        let (_retired_tx, retired_rx) = RingBuffer::<Arc<DecodedStore>>::new(4);
        swap.park(new_producer, marker_tx, retired_rx);

        sink.write(AudioPacket::Samples(vec![0.25; 8]), &mut converter())
            .unwrap();

        // The new ring got the second packet, the old one nothing more.
        let mut got = Vec::new();
        while let Ok(v) = new_consumer.pop() {
            got.push(v);
        }
        assert_eq!(got, vec![0.25; 8]);
        assert_eq!(
            old_consumer.slots(),
            20,
            "old ring untouched after the swap"
        );
        // The 10 unread old frames were credited, so a marker stamped at
        // written frame 14 (10 + 4) is due once 4 new frames are popped.
        assert_eq!(shared.consumed_frames(), 10);
        assert_eq!(written.load(Ordering::Acquire), 14);
    }

    #[test]
    fn raw_packets_are_rejected() {
        let (producer, _consumer) = RingBuffer::<f32>::new(8);
        let mut sink = RingSink::new(
            producer,
            Arc::new(AtomicU64::new(0)),
            Arc::new(RingSwap::default()),
            Arc::new(SourceRtShared::new()),
        );
        let result = sink.write(AudioPacket::Raw(vec![0u8; 4]), &mut converter());
        assert!(result.is_err());
    }
}

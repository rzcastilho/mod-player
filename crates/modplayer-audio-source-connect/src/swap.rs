// SPDX-License-Identifier: MIT OR Apache-2.0

//! `RingSwap`: the hand-off point that lets a *running* worker adopt the
//! rings of a re-attached `ConnectRtSource`.
//!
//! `SourceHost::attach` is called for every stream (re)build — device
//! change, buffer-preset change, device fallback — and builds a fresh
//! sample ring, marker ring and retirement ring for the new RT half.
//! Until 2026-09-19 their worker-side ends were only ever handed over by
//! `SourceCommand::Initialize`, which the controller sends once per
//! session, so every re-attach after registration left the new RT
//! reading rings nobody wrote ("Buffering…" forever — quickstart 008
//! M14). Now `attach()` parks them here whenever a worker already
//! exists, and the three worker-side owners (the `RingSink` inside
//! librespot's player for samples, the command loop for markers and
//! retirements) each check their slot at the top of their own work and
//! swap in place.
//!
//! Locking: every slot is a `Mutex<Option<_>>` touched by exactly two
//! non-real-time threads — the host thread that parks a new end and the
//! worker/sink thread that takes it — never by the audio callback (the
//! RT half only ever holds the *other* end of each ring). The taker uses
//! `try_lock`, so a host mid-park can never stall the sink.

use std::sync::{Arc, Mutex};

use modplayer_audio_source::DecodedStore;
use rtrb::{Consumer, Producer};

use crate::program::Marker;

/// Worker-side ring ends parked by `attach()` for a running worker.
#[derive(Default)]
pub struct RingSwap {
    sample: Mutex<Option<Producer<f32>>>,
    marker: Mutex<Option<Producer<Marker>>>,
    retired: Mutex<Option<Consumer<Arc<DecodedStore>>>>,
}

impl RingSwap {
    /// Park a full set of worker-side ends for the next pickup. A set
    /// parked earlier and never picked up is dropped (its RT half is
    /// gone too — the host only re-attaches after dropping the old
    /// processor).
    pub fn park(
        &self,
        sample: Producer<f32>,
        marker: Producer<Marker>,
        retired: Consumer<Arc<DecodedStore>>,
    ) {
        *self
            .sample
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(sample);
        *self
            .marker
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(marker);
        *self
            .retired
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(retired);
    }

    /// The parked sample producer, if any (non-blocking).
    pub fn take_sample(&self) -> Option<Producer<f32>> {
        self.sample.try_lock().ok().and_then(|mut slot| slot.take())
    }

    /// The parked marker producer, if any (non-blocking).
    pub fn take_marker(&self) -> Option<Producer<Marker>> {
        self.marker.try_lock().ok().and_then(|mut slot| slot.take())
    }

    /// The parked retirement consumer, if any (non-blocking).
    pub fn take_retired(&self) -> Option<Consumer<Arc<DecodedStore>>> {
        self.retired
            .try_lock()
            .ok()
            .and_then(|mut slot| slot.take())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;
    use rtrb::RingBuffer;

    #[test]
    fn park_then_take_hands_each_end_over_exactly_once() {
        let swap = RingSwap::default();
        assert!(swap.take_sample().is_none());
        assert!(swap.take_marker().is_none());
        assert!(swap.take_retired().is_none());

        let (sample_tx, _sample_rx) = RingBuffer::<f32>::new(8);
        let (marker_tx, _marker_rx) = RingBuffer::<Marker>::new(8);
        let (_retired_tx, retired_rx) = RingBuffer::<Arc<DecodedStore>>::new(8);
        swap.park(sample_tx, marker_tx, retired_rx);

        assert!(swap.take_sample().is_some());
        assert!(swap.take_sample().is_none(), "taken once");
        assert!(swap.take_marker().is_some());
        assert!(swap.take_marker().is_none());
        assert!(swap.take_retired().is_some());
        assert!(swap.take_retired().is_none());
    }

    #[test]
    fn a_later_park_replaces_an_unclaimed_earlier_one() {
        let swap = RingSwap::default();
        let (old_tx, old_rx) = RingBuffer::<f32>::new(8);
        let (marker_tx, _m) = RingBuffer::<Marker>::new(8);
        let (_r, retired_rx) = RingBuffer::<Arc<DecodedStore>>::new(8);
        swap.park(old_tx, marker_tx, retired_rx);

        let (new_tx, mut new_rx) = RingBuffer::<f32>::new(8);
        let (marker_tx, _m) = RingBuffer::<Marker>::new(8);
        let (_r, retired_rx) = RingBuffer::<Arc<DecodedStore>>::new(8);
        swap.park(new_tx, marker_tx, retired_rx);

        let mut taken = swap.take_sample().expect("the newer producer");
        taken.push(1.0).unwrap();
        assert_eq!(new_rx.pop().unwrap(), 1.0, "writes land in the newer ring");
        drop(old_rx);
    }
}

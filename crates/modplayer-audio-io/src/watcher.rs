// SPDX-License-Identifier: MIT OR Apache-2.0

//! Device watcher thread (US3 T069; contracts/output-backend.md): polls the
//! active device's `default_output_config()` every 500 ms for external
//! sample-rate changes, and re-enumerates every 2 s for device add/remove
//! (reappearance detection, FR-012). Runs entirely off the real-time path —
//! `CpalBackend`'s audio callback never touches this thread. Device *loss*
//! is detected separately, by `cpal`'s error callback
//! (`ErrorKind::DeviceNotAvailable`, see `cpal_backend.rs`), not by this
//! watcher: a disappeared device would otherwise just stop showing up in
//! the 2 s re-enumeration, which is too coarse a bound for FR-012's "within
//! one buffer duration".

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait};
use modplayer_engine::{DeviceId, SampleRate};

use crate::backend::BackendEvent;

const RATE_POLL_INTERVAL: Duration = Duration::from_millis(500);
const LIST_POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Owns the watcher thread for one open stream. Dropping it stops the
/// thread (join, no detached background work survives the backend that
/// spawned it).
pub struct DeviceWatcher {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl DeviceWatcher {
    /// Spawn a watcher for `device_id`, sending `BackendEvent`s on
    /// `events_tx` (the same channel `CpalBackend::events()` reads from).
    /// Builds its own `cpal::Host` on the watcher thread rather than
    /// sharing the backend's, since `cpal::Host` is not `Sync`/cheaply
    /// shareable across threads and re-querying the host is inexpensive
    /// compared to the 500 ms poll period.
    pub fn spawn(device_id: DeviceId, events_tx: Sender<BackendEvent>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_for_thread = Arc::clone(&stop);
        let handle = thread::spawn(move || watch_loop(device_id, events_tx, stop_for_thread));
        Self {
            stop,
            handle: Some(handle),
        }
    }
}

impl Drop for DeviceWatcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn watch_loop(device_id: DeviceId, events_tx: Sender<BackendEvent>, stop: Arc<AtomicBool>) {
    let host = cpal::default_host();
    let mut last_rate = current_rate(&host, &device_id);
    let mut last_device_names = enumerate_names(&host);
    let mut last_list_check = Instant::now();

    // Sleep in short slices so `Drop` doesn't have to wait up to a full
    // poll interval to observe `stop`.
    const SLEEP_SLICE: Duration = Duration::from_millis(50);

    loop {
        let mut slept = Duration::ZERO;
        while slept < RATE_POLL_INTERVAL {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            thread::sleep(SLEEP_SLICE);
            slept += SLEEP_SLICE;
        }
        if stop.load(Ordering::Relaxed) {
            return;
        }

        if let Some(rate) = current_rate(&host, &device_id)
            && last_rate != Some(rate)
        {
            last_rate = Some(rate);
            let _ = events_tx.send(BackendEvent::SampleRateChanged {
                id: device_id.clone(),
                new_rate: rate,
            });
        }

        if last_list_check.elapsed() >= LIST_POLL_INTERVAL {
            last_list_check = Instant::now();
            let names = enumerate_names(&host);
            if names != last_device_names {
                last_device_names = names;
                let _ = events_tx.send(BackendEvent::DeviceListChanged);
            }
        }
    }
}

/// The current sample rate `device_id` reports, if it can still be found.
fn current_rate(host: &cpal::Host, device_id: &DeviceId) -> Option<SampleRate> {
    let devices = host.output_devices().ok()?;
    for device in devices {
        if super::cpal_backend::device_id_matches(&device, device_id) {
            let config = device.default_output_config().ok()?;
            return Some(SampleRate::new(config.sample_rate()));
        }
    }
    None
}

/// A cheap fingerprint of the current output device list (names — good
/// enough to detect add/remove for `DeviceListChanged`; avoids re-deriving
/// full `OutputDeviceInfo` off the real-time path every 2 s).
fn enumerate_names(host: &cpal::Host) -> Vec<String> {
    let Ok(devices) = host.output_devices() else {
        return Vec::new();
    };
    let mut names: Vec<String> = devices.map(|d| d.to_string()).collect();
    names.sort();
    names
}

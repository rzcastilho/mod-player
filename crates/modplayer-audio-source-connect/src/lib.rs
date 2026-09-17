// SPDX-License-Identifier: MIT OR Apache-2.0
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used)]

//! The Connect receiver crate (contracts/connect-source.md): wraps
//! librespot 0.8 (`Session` + `Spirc` + `Player`) behind the `SourceHost`
//! seam so `crates/modplayer-audio-source` and every other crate stay free
//! of the Connect protocol (Constitution IV). Only this crate imports
//! `librespot-*`; only `crates/modplayer` depends on this crate.
//!
//! **Deviation from contracts/connect-source.md §1** (recorded here per
//! Constitution X): `ConnectConfig::device_name` is a plain `String`
//! (already resolved to the effective name by the caller) rather than
//! `modplayer_core::settings::DeviceName` — this crate must stay free of
//! `modplayer-core` (plan.md's dependency graph:
//! `audio-source-connect -> {audio-source, librespot-*, tokio, rtrb}`).
//! Likewise `tmp_dir` is not part of `ConnectConfig`: this crate creates
//! and purges its own private temp directory (`tmp.rs`) rather than
//! taking one from the caller, keeping temp-directory hygiene the
//! receiver's own concern (contract §6).

pub mod catalog;
mod credentials;
mod events;
mod health;
mod mixer;
mod program;
mod rt;
mod sink;
mod tmp;
mod worker;

use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;

use modplayer_audio_source::{
    BufferStatus, Program, SourceCommand, SourceEvent, SourceHealth, SourceHost, SourceRtShared,
};
use rtrb::RingBuffer;

pub use credentials::{CredentialError, ReceiverCredentials};
pub use rt::ConnectRtSource;

/// Stereo `f32` ring capacity, in samples (8 192 frames — contract §5).
const RING_CAPACITY: usize = 16_384;
/// Marker ring capacity (contract §5).
const MARKER_CAPACITY: usize = 64;
/// `Shutdown`'s "synchronous best effort" budget (contract §2).
const SHUTDOWN_JOIN_TIMEOUT: Duration = Duration::from_secs(2);
/// `BufferStatus::ready` threshold: at least a quarter-second of stereo
/// audio buffered at the fixed 44.1 kHz decode rate.
const READY_THRESHOLD_FRAMES: u32 = rt::SAMPLE_RATE / 4;

/// Set librespot's process-global read-ahead parameters once (research R5):
/// `read_ahead_before_playback = 2 s` (SC-002's un-streamed-track
/// readiness threshold) and `read_ahead_during_playback = 1 h` (FR-009's
/// "as fast as the connection allows" pre-buffer of the current track —
/// effectively "fetch the whole file ahead of the read position").
/// `AudioFetchParams::set` is a `OnceLock`; a later call (e.g. a second
/// `ConnectSource` in the same process, which never happens in production
/// but does in tests) is silently ignored rather than panicking.
fn configure_audio_fetch_params() {
    let _ = librespot_audio::AudioFetchParams::set(librespot_audio::AudioFetchParams {
        read_ahead_before_playback: Duration::from_secs(2),
        read_ahead_during_playback: Duration::from_secs(60 * 60),
        ..librespot_audio::AudioFetchParams::default()
    });
}

/// Construction parameters for [`ConnectSource`] (contracts/connect-
/// source.md §1, with the deviations noted in the module doc above).
pub struct ConnectConfig {
    pub device_name: String,
    pub device_id: String,
    pub credentials: Arc<dyn ReceiverCredentials>,
}

impl std::fmt::Debug for ConnectConfig {
    /// Redacting: never prints anything token-shaped (design note 5's
    /// "Debug impls redact"; the crate's own credential-leak coverage
    /// lives in the binary's `tests/single_dependent.rs` sibling).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectConfig")
            .field("device_name", &self.device_name)
            .field("device_id", &self.device_id)
            .field("credentials", &"<redacted>")
            .finish()
    }
}

/// The Connect receiver: `impl SourceHost`, `Rt = ConnectRtSource`
/// (contracts/connect-source.md §1).
pub struct ConnectSource {
    device_name: String,
    device_id: String,
    credentials: Arc<dyn ReceiverCredentials>,
    shared: Arc<SourceRtShared>,
    event_tx: Sender<SourceEvent>,
    event_rx: Receiver<SourceEvent>,
    worker: Option<worker::WorkerHandles>,
    pending_sample_tx: Option<rtrb::Producer<f32>>,
    pending_marker_tx: Option<rtrb::Producer<Marker>>,
    /// The last `LoadProgram` sent, resent on `SetDeviceName`'s
    /// re-registration and on a fresh worker (re)spawn (contract §2).
    last_program: Option<Program>,
    health: SourceHealth,
    tmp_dir: Option<std::path::PathBuf>,
    initial_volume_pct: u8,
}

use crate::program::Marker;

impl ConnectSource {
    /// No I/O; the worker thread starts on `Initialize`
    /// (contracts/connect-source.md §1).
    pub fn new(config: ConnectConfig) -> Self {
        configure_audio_fetch_params();
        let (event_tx, event_rx) = std::sync::mpsc::channel();
        Self {
            device_name: config.device_name,
            device_id: config.device_id,
            credentials: config.credentials,
            shared: Arc::new(SourceRtShared::new()),
            event_tx,
            event_rx,
            worker: None,
            pending_sample_tx: None,
            pending_marker_tx: None,
            last_program: None,
            health: SourceHealth::Ok,
            tmp_dir: None,
            initial_volume_pct: 50,
        }
    }

    pub const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    fn handle_initialize(&mut self, device_name: String, device_id: String) {
        if self.worker.is_some() {
            return;
        }
        self.device_name = device_name;
        self.device_id = device_id;
        let (Some(sample_tx), Some(marker_tx)) =
            (self.pending_sample_tx.take(), self.pending_marker_tx.take())
        else {
            // `attach()` (which builds the ring) must run before
            // `Initialize` is ever sent — the controller always opens its
            // stream at launch, before any account permission is known.
            // Defensive fallback for an out-of-order call.
            let _ = self
                .event_tx
                .send(SourceEvent::Health(SourceHealth::Unavailable {
                    client_update_required: false,
                }));
            return;
        };
        let tmp_dir = tmp::prepare(std::process::id());
        self.tmp_dir = Some(tmp_dir.clone());
        let worker_config = worker::WorkerConfig {
            device_id: self.device_id.clone(),
            tmp_dir,
            credentials: Arc::clone(&self.credentials),
            initial_volume_pct: self.initial_volume_pct,
        };
        self.worker = worker::spawn(
            self.device_name.clone(),
            worker_config,
            self.event_tx.clone(),
            sample_tx,
            marker_tx,
            Arc::clone(&self.shared),
            self.last_program.clone(),
        );
        if self.worker.is_none() {
            let _ = self
                .event_tx
                .send(SourceEvent::Health(SourceHealth::Unavailable {
                    client_update_required: false,
                }));
        }
    }
}

impl SourceHost for ConnectSource {
    type Rt = ConnectRtSource;

    fn attach(&mut self, position_frames: u64) -> Self::Rt {
        let (sample_tx, sample_rx) = RingBuffer::<f32>::new(RING_CAPACITY);
        let (marker_tx, marker_rx) = RingBuffer::<Marker>::new(MARKER_CAPACITY);
        self.pending_sample_tx = Some(sample_tx);
        self.pending_marker_tx = Some(marker_tx);
        ConnectRtSource::new(
            sample_rx,
            marker_rx,
            Arc::clone(&self.shared),
            position_frames,
        )
    }

    fn command(&mut self, cmd: SourceCommand) {
        match cmd {
            SourceCommand::Initialize {
                device_name,
                device_id,
            } => {
                self.handle_initialize(device_name, device_id);
            }
            SourceCommand::Shutdown => {
                if let Some(mut worker) = self.worker.take() {
                    worker.send(SourceCommand::Shutdown);
                    worker.join_with_timeout(SHUTDOWN_JOIN_TIMEOUT);
                } else if let Some(dir) = self.tmp_dir.take() {
                    tmp::purge(&dir);
                }
            }
            SourceCommand::LoadProgram(ref program) => {
                self.last_program = Some(program.clone());
                if let Some(worker) = &self.worker {
                    worker.send(cmd);
                }
            }
            other => {
                if let Some(worker) = &self.worker {
                    worker.send(other);
                }
            }
        }
    }

    fn poll(&mut self) -> Vec<SourceEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.event_rx.try_recv() {
            if let SourceEvent::Health(health) = &event {
                self.health = health.clone();
            }
            events.push(event);
        }
        events
    }

    fn buffer_status(&self) -> BufferStatus {
        let ring_fill_frames = self.shared.ring_fill_frames();
        BufferStatus {
            ring_fill_frames,
            ready: ring_fill_frames >= READY_THRESHOLD_FRAMES,
            current_prefetched: false,
            next: None,
        }
    }

    fn health(&self) -> SourceHealth {
        self.health.clone()
    }

    fn rt_shared(&self) -> Arc<SourceRtShared> {
        Arc::clone(&self.shared)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StaticCredentials;
    impl ReceiverCredentials for StaticCredentials {
        fn access_token(&self) -> Result<String, CredentialError> {
            Ok("test-token".to_string())
        }
    }

    fn source() -> ConnectSource {
        ConnectSource::new(ConnectConfig {
            device_name: "Test Device".to_string(),
            device_id: "0".repeat(32),
            credentials: Arc::new(StaticCredentials),
        })
    }

    #[test]
    fn debug_never_prints_the_device_name_as_a_credential_placeholder() {
        let config = ConnectConfig {
            device_name: "My Room".to_string(),
            device_id: "a".repeat(32),
            credentials: Arc::new(StaticCredentials),
        };
        let debug = format!("{config:?}");
        assert!(debug.contains("My Room"));
        assert!(debug.contains("<redacted>"));
    }

    #[test]
    fn attach_returns_an_rt_source_at_the_requested_position() {
        let mut source = source();
        let rt = source.attach(1_234);
        assert_eq!(modplayer_audio_source::AudioSource::position(&rt), 1_234);
    }

    #[test]
    fn initialize_without_attach_reports_unavailable_instead_of_panicking() {
        let mut source = source();
        source.command(SourceCommand::Initialize {
            device_name: "Test".to_string(),
            device_id: "1".repeat(32),
        });
        let events = source.poll();
        assert!(matches!(
            events.first(),
            Some(SourceEvent::Health(SourceHealth::Unavailable { .. }))
        ));
    }

    #[test]
    fn buffer_status_reports_not_ready_before_any_data() {
        let source = source();
        let status = source.buffer_status();
        assert!(!status.ready);
        assert_eq!(status.ring_fill_frames, 0);
    }

    #[test]
    fn shutdown_without_a_worker_is_a_harmless_no_op() {
        let mut source = source();
        source.command(SourceCommand::Shutdown);
        assert!(source.poll().is_empty());
    }
}

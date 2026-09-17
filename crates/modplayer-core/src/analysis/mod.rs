// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Analysis Service: folds a track's `DecodedStore` into cached,
//! multi-level waveform peaks for the Now Playing waveform, off the
//! real-time path, at below-normal OS priority
//! (005-now-playing-waveform, contracts/analysis-service.md §1,
//! data-model.md §3). `peaks`, `cache` and `worker` are its sub-modules.

pub mod cache;
pub mod peaks;
pub mod worker;

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use modplayer_audio_source::{DecodedStore, PeakBucket, TrackId};

pub use cache::{ANALYSIS_DIR_ENV, AnalysisPaths, CacheError};

/// Bumped by hand on any peak/format change (FR-007); written to and
/// compared against every `.mpwf` entry.
pub const ANALYZER_VERSION: u32 = 1;

/// `AnalysisService::shutdown`'s join budget (mirrors the receiver
/// worker's `Shutdown` pattern).
const SHUTDOWN_JOIN_TIMEOUT: Duration = Duration::from_secs(2);

/// A track's analysis lifecycle (data-model.md §3.1). `Complete` and
/// `Failed` are terminal for the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalysisStatus {
    Pending,
    Partial,
    Complete,
    Failed,
}

/// One ladder level's folded peaks (data-model.md §3.2). `present` is a
/// bitmap (1 word per 64 buckets); a bucket is present iff its bit is set.
#[derive(Debug, Clone, PartialEq)]
pub struct PeakLevel {
    pub frames_per_bucket: u32,
    pub buckets: Vec<PeakBucket>,
    present: Vec<u64>,
}

impl PeakLevel {
    /// An all-`Pending` level shell of `bucket_count` buckets.
    pub(crate) fn empty(frames_per_bucket: u32, bucket_count: usize) -> Self {
        Self {
            frames_per_bucket,
            buckets: vec![PeakBucket::default(); bucket_count],
            present: vec![0u64; bucket_count.div_ceil(64)],
        }
    }

    /// Every bucket marked present: only a `Complete` ladder is ever
    /// written to disk, so a cache load always reconstructs a full bitmap
    /// (data-model.md §6) rather than storing one. `pub` (not
    /// `pub(crate)`) so `cache`'s round-trip proptest, run from
    /// `tests/analysis.rs`, can build arbitrary `WaveformPeaks` values —
    /// every value this constructor can produce is exactly the shape a
    /// real cache entry always has.
    pub fn full(frames_per_bucket: u32, buckets: Vec<PeakBucket>) -> Self {
        let bucket_count = buckets.len();
        let mut level = Self {
            frames_per_bucket,
            buckets,
            present: vec![0u64; bucket_count.div_ceil(64)],
        };
        for index in 0..bucket_count {
            level.set_present(index);
        }
        level
    }

    /// A level with only `present` buckets marked (the rest left at their
    /// default `PeakBucket`) — for cross-crate tests that need a specific
    /// presence gap (e.g. `modplayer-ui`'s waveform-painting unit tests,
    /// which exercise the "any absent bucket -> placeholder" rule without
    /// driving a real `AnalysisService`). `pub` for the same reason as
    /// [`Self::full`].
    pub fn partial(
        frames_per_bucket: u32,
        buckets: Vec<PeakBucket>,
        present: impl IntoIterator<Item = usize>,
    ) -> Self {
        let mut level = Self::empty(frames_per_bucket, buckets.len());
        level.buckets = buckets;
        for index in present {
            level.set_present(index);
        }
        level
    }

    pub fn is_present(&self, bucket: usize) -> bool {
        let word = bucket / 64;
        let bit = bucket % 64;
        self.present.get(word).is_some_and(|w| (w >> bit) & 1 == 1)
    }

    pub(crate) fn set_present(&mut self, bucket: usize) {
        let word = bucket / 64;
        let bit = bucket % 64;
        if let Some(w) = self.present.get_mut(word) {
            *w |= 1 << bit;
        }
    }

    pub fn len(&self) -> usize {
        self.buckets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buckets.is_empty()
    }

    /// Count of present buckets — also this level's fold watermark, since
    /// folding only ever proceeds left to right without gaps (level 0 from
    /// `DecodedStore::fold_peaks`'s own "stops at the first incomplete
    /// bucket" rule; coarser levels the same way in `peaks::
    /// refold_coarser_levels`).
    pub(crate) fn present_count(&self) -> usize {
        (0..self.len())
            .filter(|&index| self.is_present(index))
            .count()
    }
}

/// A track's full multi-level peak ladder (data-model.md §3.2).
#[derive(Debug, Clone, PartialEq)]
pub struct WaveformPeaks {
    pub sample_rate: u32,
    pub len_frames: u64,
    pub levels: Vec<PeakLevel>,
}

impl WaveformPeaks {
    pub const LADDER: [u32; 4] = [128, 1_024, 8_192, 65_536];

    /// The coarsest level whose `frames_per_bucket <= frames_per_pixel`
    /// (the fewest buckets a column needs to fold while still resolving at
    /// least one bucket per pixel); level 0 if none qualify — i.e. the
    /// view is zoomed in past even the finest level's resolution
    /// (contracts/analysis-service.md §1).
    pub fn level_for(&self, frames_per_pixel: f64) -> &PeakLevel {
        self.levels
            .iter()
            .rev()
            .find(|level| f64::from(level.frames_per_bucket) <= frames_per_pixel)
            .unwrap_or(&self.levels[0])
    }
}

/// One immutable analysis result, `Arc`-shared with the UI without locks
/// (data-model.md §3.3).
#[derive(Debug, Clone, PartialEq)]
pub struct AnalysisSnapshot {
    pub track: TrackId,
    pub analyzer_version: u32,
    pub status: AnalysisStatus,
    /// `None` while `Pending`, for a silent-track `Failed`, and for
    /// `Failed` before any bucket existed.
    pub peaks: Option<Arc<WaveformPeaks>>,
    /// `true` when `Complete` came from the on-disk cache (SC-004).
    pub from_cache: bool,
}

/// Commands from `AnalysisService` to the `analysis` thread.
#[derive(Debug)]
pub(crate) enum AnalysisCommand {
    Attach {
        track: TrackId,
        sample_rate: u32,
        len_frames: u64,
    },
    AttachStore {
        track: TrackId,
        store: Arc<DecodedStore>,
    },
    Detach,
    Shutdown,
}

/// The Analysis Service (contracts/analysis-service.md §1), owned by
/// `PlaybackController`. Every disk/decode access runs on its dedicated
/// `analysis` thread (A13); this handle only sends commands and drains
/// published snapshots — never blocks the caller.
pub struct AnalysisService {
    commands: Sender<AnalysisCommand>,
    progress: Receiver<Arc<AnalysisSnapshot>>,
    latest: Option<Arc<AnalysisSnapshot>>,
    thread: Option<JoinHandle<()>>,
}

impl AnalysisService {
    /// Spawn the `analysis` thread. `paths: None` (an unresolvable data
    /// dir) disables the on-disk cache only — analysis still runs in
    /// memory.
    pub fn new(paths: Option<AnalysisPaths>) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (progress_tx, progress_rx) = mpsc::channel();
        let builder = thread::Builder::new().name("analysis".to_string());
        let thread = builder
            .spawn(move || worker::run(cmd_rx, progress_tx, paths))
            .ok();
        Self {
            commands: cmd_tx,
            progress: progress_rx,
            latest: None,
            thread,
        }
    }

    /// Attach a new current track (contracts/transport-delta.md §2). A
    /// no-op if `track` is already the current attachment (A10, in-flight
    /// analysis reused).
    pub fn attach(&mut self, track: TrackId, sample_rate: u32, len_frames: u64) {
        self.latest = None;
        let _ = self.commands.send(AnalysisCommand::Attach {
            track,
            sample_rate,
            len_frames,
        });
    }

    /// Hand the current track's decoded store to the thread; ignored
    /// there if `track` is not the current attachment.
    pub fn attach_store(&mut self, track: TrackId, store: Arc<DecodedStore>) {
        let _ = self
            .commands
            .send(AnalysisCommand::AttachStore { track, store });
    }

    /// Drop the current attachment. Nothing incomplete is ever written
    /// (A9, FR-017).
    pub fn detach(&mut self) {
        self.latest = None;
        let _ = self.commands.send(AnalysisCommand::Detach);
    }

    /// Drain every snapshot published since the last call, keeping only
    /// the last. Returns `true` if `latest()` changed.
    pub fn drain(&mut self) -> bool {
        let mut changed = false;
        while let Ok(snapshot) = self.progress.try_recv() {
            self.latest = Some(snapshot);
            changed = true;
        }
        changed
    }

    /// The most recently drained snapshot, if any.
    pub fn latest(&self) -> Option<&Arc<AnalysisSnapshot>> {
        self.latest.as_ref()
    }

    /// Join the thread with a 2 s budget (mirrors the receiver worker's
    /// `Shutdown` pattern). `&mut self` rather than the contract's `self`
    /// (contracts/analysis-service.md §1) so `PlaybackController::
    /// shutdown(&mut self)` can call it on its owned field without a
    /// placeholder swap; idempotent (a second call finds `thread` already
    /// taken and is a no-op beyond the harmless resend).
    pub fn shutdown(&mut self) {
        let _ = self.commands.send(AnalysisCommand::Shutdown);
        if let Some(thread) = self.thread.take() {
            let start = Instant::now();
            while !thread.is_finished() && start.elapsed() < SHUTDOWN_JOIN_TIMEOUT {
                thread::sleep(Duration::from_millis(10));
            }
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peak_level_present_bitmap_round_trips() {
        let mut level = PeakLevel::empty(128, 130);
        assert!(!level.is_present(0));
        level.set_present(0);
        level.set_present(64);
        level.set_present(129);
        assert!(level.is_present(0));
        assert!(level.is_present(64));
        assert!(level.is_present(129));
        assert!(!level.is_present(1));
        assert_eq!(level.present_count(), 3);
    }

    #[test]
    fn level_for_picks_coarsest_level_within_budget() {
        let peaks = WaveformPeaks {
            sample_rate: 44_100,
            len_frames: 44_100 * 600,
            levels: WaveformPeaks::LADDER
                .iter()
                .map(|&fpb| PeakLevel::empty(fpb, 4))
                .collect(),
        };
        assert_eq!(peaks.level_for(1.0).frames_per_bucket, 128);
        assert_eq!(peaks.level_for(200.0).frames_per_bucket, 128);
        assert_eq!(peaks.level_for(1_500.0).frames_per_bucket, 1_024);
        assert_eq!(peaks.level_for(100_000.0).frames_per_bucket, 65_536);
    }
}

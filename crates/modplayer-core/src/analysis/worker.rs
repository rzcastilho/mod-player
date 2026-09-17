// SPDX-License-Identifier: MIT OR Apache-2.0

//! The analysis thread loop: cache lookup, bounded-pass peak folding with
//! a publish cadence, terminal `Complete`/`Failed` handling, and
//! below-normal thread priority (005-now-playing-waveform, rules A1-A13).

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use modplayer_audio_source::{DecodedStore, StoreState, TrackId};

use super::{
    AnalysisCommand, AnalysisPaths, AnalysisSnapshot, AnalysisStatus, WaveformPeaks, cache, peaks,
};

/// Idle poll interval (also bounds how often a `Filling` attachment gets a
/// fold pass — research R5 step 3's "park 50 ms when nothing new").
const IDLE_PARK: Duration = Duration::from_millis(50);
/// Maximum interval between `Partial` publishes while folding (SC-002).
const PUBLISH_INTERVAL: Duration = Duration::from_millis(250);

struct Attachment {
    track: TrackId,
    peaks: WaveformPeaks,
    store: Option<Arc<DecodedStore>>,
    last_publish: Instant,
    /// Set whenever a fold produces buckets not yet published, cleared on
    /// publish — tracked across calls (not just the current one) so a
    /// burst of folding that lands *before* `PUBLISH_INTERVAL` has elapsed
    /// is not lost: the next `step` that finds the interval due still
    /// publishes it even if nothing new folded in that same call (SC-002:
    /// "a snapshot's present buckets never lag the store's coverage by
    /// more than 2s", not "only if a fold happens in the exact same tick
    /// the interval expires").
    dirty: bool,
    /// Set once a `Complete`/`Failed` snapshot has been published for this
    /// attachment, so `step` never republishes it every idle tick.
    terminal_published: bool,
}

/// The `analysis` thread entry point (contracts/analysis-service.md §1,
/// rules A1-A13). `paths: None` disables the on-disk cache only.
pub(crate) fn run(
    commands: Receiver<AnalysisCommand>,
    progress: Sender<Arc<AnalysisSnapshot>>,
    paths: Option<AnalysisPaths>,
) {
    if let Err(error) =
        thread_priority::set_current_thread_priority(thread_priority::ThreadPriority::Min)
    {
        // Non-fatal (R9/A12): the work is still chunked/bounded per pass.
        let _ = error;
    }

    let mut attached: Option<Attachment> = None;
    let mut failed: HashSet<TrackId> = HashSet::new();

    loop {
        match commands.recv_timeout(IDLE_PARK) {
            Ok(AnalysisCommand::Shutdown) => break,
            Ok(AnalysisCommand::Attach {
                track,
                sample_rate,
                len_frames,
            }) => {
                handle_attach(
                    &mut attached,
                    &failed,
                    &paths,
                    &progress,
                    track,
                    sample_rate,
                    len_frames,
                );
            }
            Ok(AnalysisCommand::AttachStore { track, store }) => {
                if let Some(att) = attached.as_mut()
                    && att.track == track
                {
                    att.store = Some(store);
                }
            }
            Ok(AnalysisCommand::Detach) => {
                attached = None; // A9: drops the store/partial peaks, writes nothing
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }

        if let Some(att) = attached.as_mut() {
            step(att, &mut failed, &paths, &progress);
        }
    }
}

fn handle_attach(
    attached: &mut Option<Attachment>,
    failed: &HashSet<TrackId>,
    paths: &Option<AnalysisPaths>,
    progress: &Sender<Arc<AnalysisSnapshot>>,
    track: TrackId,
    sample_rate: u32,
    len_frames: u64,
) {
    if attached.as_ref().is_some_and(|att| att.track == track) {
        return; // A10: same track already attached, in-flight analysis reused
    }

    if failed.contains(&track) {
        publish(progress, &track, AnalysisStatus::Failed, None, false);
        *attached = None;
        return;
    }

    if let Some(paths) = paths
        && let Some(loaded) = cache::load(paths, &track, len_frames)
    {
        publish(
            progress,
            &track,
            AnalysisStatus::Complete,
            Some(Arc::new(loaded)),
            true,
        );
        *attached = None; // A1: nothing more to do, a store is never read
        return;
    }

    publish(progress, &track, AnalysisStatus::Pending, None, false);
    *attached = Some(Attachment {
        track,
        peaks: peaks::empty_peaks(sample_rate, len_frames),
        store: None,
        last_publish: Instant::now(),
        dirty: false,
        terminal_published: false,
    });
}

fn step(
    att: &mut Attachment,
    failed: &mut HashSet<TrackId>,
    paths: &Option<AnalysisPaths>,
    progress: &Sender<Arc<AnalysisSnapshot>>,
) {
    if att.terminal_published {
        return;
    }
    let Some(store) = att.store.clone() else {
        return; // A4: no store yet, stays Pending
    };

    let mut folded_any = false;
    loop {
        let written = peaks::fold_level0(&mut att.peaks, &store);
        if written == 0 {
            break;
        }
        folded_any = true;
        peaks::refold_coarser_levels(&mut att.peaks);
    }
    if folded_any {
        att.dirty = true;
    }

    match store.state() {
        StoreState::Complete => {
            att.terminal_published = true;
            if peaks::is_silent(&att.peaks) {
                failed.insert(att.track.clone());
                publish(progress, &att.track, AnalysisStatus::Failed, None, false);
            } else {
                if let Some(paths) = paths {
                    // Best-effort: a write failure leaves no cache entry
                    // but the in-memory `Complete` snapshot still
                    // publishes (analysis correctness never depends on
                    // disk succeeding).
                    let _ = cache::store(paths, &att.track, &att.peaks);
                }
                publish(
                    progress,
                    &att.track,
                    AnalysisStatus::Complete,
                    Some(Arc::new(att.peaks.clone())),
                    false,
                );
            }
        }
        StoreState::Failed => {
            att.terminal_published = true;
            failed.insert(att.track.clone());
            let has_any = att
                .peaks
                .levels
                .first()
                .is_some_and(|level| level.present_count() > 0);
            let peaks = has_any.then(|| Arc::new(att.peaks.clone()));
            publish(progress, &att.track, AnalysisStatus::Failed, peaks, false);
        }
        StoreState::Filling => {
            let due = att.last_publish.elapsed() >= PUBLISH_INTERVAL;
            if att.dirty && due {
                att.last_publish = Instant::now();
                att.dirty = false;
                publish(
                    progress,
                    &att.track,
                    AnalysisStatus::Partial,
                    Some(Arc::new(att.peaks.clone())),
                    false,
                );
            }
        }
    }
}

fn publish(
    progress: &Sender<Arc<AnalysisSnapshot>>,
    track: &TrackId,
    status: AnalysisStatus,
    peaks: Option<Arc<WaveformPeaks>>,
    from_cache: bool,
) {
    let _ = progress.send(Arc::new(AnalysisSnapshot {
        track: track.clone(),
        analyzer_version: super::ANALYZER_VERSION,
        status,
        peaks,
        from_cache,
    }));
}

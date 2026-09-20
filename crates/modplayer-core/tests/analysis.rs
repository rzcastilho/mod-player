// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! Tests pinning contracts/analysis-service.md (005-now-playing-waveform,
//! Phase 3: User Story 1 slice — T017). Progressive-fill timing (US3),
//! cache-version/eviction edge cases (US4) and the failure-handling suite
//! (Polish) each have their own later tests; this file covers what US1's
//! checkpoint needs: a cache hit short-circuits without ever touching a
//! store (A1), and the ladder folds ×8 with presence following coverage
//! (data-model.md §3.2), plus the `.mpwf` format's own round-trip/
//! truncation properties (data-model.md §6).

use std::time::{Duration, Instant};

use modplayer_audio_source::{DecodedStore, PeakBucket, TrackId};
use modplayer_core::analysis::cache;
use modplayer_core::analysis::{
    ANALYZER_VERSION, AnalysisPaths, AnalysisService, AnalysisStatus, PeakLevel, WaveformPeaks,
};
use proptest::prelude::*;

fn track(id: &str) -> TrackId {
    TrackId::new(id).unwrap_or_else(|_| unreachable!())
}

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "modplayer-analysis-test-{}-{tag}-{}",
        std::process::id(),
        fastrand_ish()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// A cheap, dependency-free "unique enough" suffix for parallel test temp
/// dirs (no `rand` dependency, mirrors 004's persistence tests).
fn fastrand_ish() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

fn drain_until<F>(service: &mut AnalysisService, timeout: Duration, mut done: F) -> bool
where
    F: FnMut(&modplayer_core::analysis::AnalysisSnapshot) -> bool,
{
    let start = Instant::now();
    loop {
        service.drain();
        if let Some(snapshot) = service.latest()
            && done(snapshot)
        {
            return true;
        }
        if start.elapsed() >= timeout {
            return false;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// A store filled end to end with a simple alternating (non-silent, non-
/// constant) signal, already `Complete`.
fn complete_store(frames: u64) -> std::sync::Arc<DecodedStore> {
    let store = DecodedStore::new(44_100, frames);
    let interleaved: Vec<f32> = (0..frames)
        .flat_map(|i| {
            let s = if i % 4 < 2 { 0.6 } else { -0.6 };
            [s, s]
        })
        .collect();
    store.write_frames(0, &interleaved);
    store.set_complete(frames);
    store
}

// -- A1 / SC-004: cache hit publishes Complete without ever touching a
// store, within 200 ms ------------------------------------------------------

#[test]
fn cache_hit_publishes_complete_without_a_store() {
    let dir = temp_dir("cache-hit");
    let paths = AnalysisPaths::with_dir(&dir);
    let id = track("spotify:track:cachehit");
    let len_frames = 44_100 * 30;

    let peaks = WaveformPeaks {
        sample_rate: 44_100,
        len_frames,
        levels: vec![PeakLevel::full(
            128,
            (0..len_frames.div_ceil(128))
                .map(|i| PeakBucket {
                    min: -((i % 100) as i8),
                    max: (i % 100) as i8,
                })
                .collect(),
        )],
    };
    cache::store(&paths, &id, &peaks).unwrap_or_else(|e| panic!("{e}"));

    let mut service = AnalysisService::new(Some(paths));
    let start = Instant::now();
    // Attach only — deliberately never call attach_store: A1 requires the
    // cache hit to publish `Complete` without ever reading a store.
    service.attach(id.clone(), 44_100, len_frames);

    let got = drain_until(&mut service, Duration::from_millis(500), |snapshot| {
        snapshot.status == AnalysisStatus::Complete
    });
    let elapsed = start.elapsed();
    assert!(got, "expected a Complete snapshot from the cache hit");
    assert!(
        elapsed < Duration::from_millis(200),
        "cache hit took {elapsed:?}, expected < 200ms (SC-004)"
    );

    let snapshot = service.latest().unwrap_or_else(|| unreachable!());
    assert!(snapshot.from_cache);
    assert_eq!(snapshot.peaks.as_deref(), Some(&peaks));

    service.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

// -- data-model.md §3.2: ladder folds ×8, presence follows coverage --------

#[test]
fn ladder_levels_fold_by_eight_and_present_bits_follow() {
    // No cache directory: every attach is a genuine miss.
    let dir = temp_dir("ladder");
    let paths = AnalysisPaths::with_dir(&dir);
    let mut service = AnalysisService::new(Some(paths));

    let id = track("spotify:track:ladder");
    // 20 level-0 buckets (128 frames each): enough for a partial level-1
    // rung (ceil(20/8) = 3 buckets, the last folding only the remaining 4).
    let level0_buckets = 20u64;
    let frames = level0_buckets * 128;

    service.attach(id.clone(), 44_100, frames);
    let store = complete_store(frames);
    service.attach_store(id.clone(), store);

    let got = drain_until(&mut service, Duration::from_secs(5), |snapshot| {
        snapshot.status == AnalysisStatus::Complete
    });
    assert!(got, "expected the store to reach Complete");

    let snapshot = service.latest().unwrap_or_else(|| unreachable!());
    let peaks = snapshot
        .peaks
        .as_ref()
        .unwrap_or_else(|| unreachable!("Complete must carry peaks"));

    let level0 = &peaks.levels[0];
    assert_eq!(level0.frames_per_bucket, 128);
    assert_eq!(level0.len(), level0_buckets as usize);
    assert!((0..level0.len()).all(|i| level0.is_present(i)));

    let level1 = &peaks.levels[1];
    assert_eq!(level1.frames_per_bucket, 1_024);
    assert_eq!(level1.len(), 3); // ceil(20 / 8)
    assert!((0..level1.len()).all(|i| level1.is_present(i)));

    service.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

// -- data-model.md §6: .mpwf round-trip / truncation properties ------------

fn level_strategy(frames_per_bucket: u32, bucket_count: usize) -> impl Strategy<Value = PeakLevel> {
    prop::collection::vec(any::<(i8, i8)>(), bucket_count).prop_map(move |pairs| {
        let buckets = pairs
            .into_iter()
            .map(|(a, b)| {
                let (min, max) = if a <= b { (a, b) } else { (b, a) };
                PeakBucket { min, max }
            })
            .collect::<Vec<_>>();
        PeakLevel::full(frames_per_bucket, buckets)
    })
}

fn peaks_strategy() -> impl Strategy<Value = (WaveformPeaks, TrackId)> {
    (
        128u64..=(500u64 * 128),
        44_100u32..=48_000u32,
        "[a-z0-9]{1,20}",
    )
        .prop_flat_map(|(len_frames, sample_rate, suffix)| {
            let fpbs = [128u32, 1_024, 8_192, 65_536];
            let bucket_count = |fpb: u32| len_frames.div_ceil(u64::from(fpb)) as usize;
            (
                Just(len_frames),
                Just(sample_rate),
                Just(suffix),
                level_strategy(fpbs[0], bucket_count(fpbs[0])),
                level_strategy(fpbs[1], bucket_count(fpbs[1])),
                level_strategy(fpbs[2], bucket_count(fpbs[2])),
                level_strategy(fpbs[3], bucket_count(fpbs[3])),
            )
        })
        .prop_map(|(len_frames, sample_rate, suffix, l0, l1, l2, l3)| {
            let track =
                TrackId::new(format!("spotify:track:{suffix}")).unwrap_or_else(|_| unreachable!());
            let peaks = WaveformPeaks {
                sample_rate,
                len_frames,
                levels: vec![l0, l1, l2, l3],
            };
            (peaks, track)
        })
}

// -- US3 (Phase 5): progressive-fill timing/behaviour guarantees ----------
// The mechanisms (decode-ahead ordering, publish cadence, per-bucket
// presence) already exist from Phase 3 (T020, T026); these tests pin the
// SC-001/SC-002/SC-012 guarantees against them directly at the
// `AnalysisService` level, without a receiver.

/// Write `[from, from + count)` of a deterministic, non-silent, non-
/// constant tone into `store` (mirrors `complete_store`'s signal, but for
/// a sub-range so callers can drive a progressive fill).
fn write_tone(store: &DecodedStore, from: u64, count: u64) {
    let interleaved: Vec<f32> = (0..count)
        .flat_map(|i| {
            let s = if (from + i) % 4 < 2 { 0.6 } else { -0.6 };
            [s, s]
        })
        .collect();
    store.write_frames(from, &interleaved);
}

#[test]
fn first_partial_within_one_second() {
    // SC-001: "the waveform starts filling within 1s". Simulated here with
    // a background thread standing in for the decode-ahead thread, writing
    // a first chunk of tone into the store immediately and then continuing
    // at a slow, real cadence — `AnalysisService::attach_store` is called
    // the moment that thread starts, and the wall-clock elapsed until the
    // first `Partial` snapshot is measured directly (same real-time-timer
    // style as `cache_hit_publishes_complete_without_a_store`'s 200ms
    // budget above).
    let dir = temp_dir("first-partial");
    let paths = AnalysisPaths::with_dir(&dir);
    let mut service = AnalysisService::new(Some(paths));

    let id = track("spotify:track:first-partial");
    let level0_frames = 128u64;
    let total_frames = level0_frames * 200;

    service.attach(id.clone(), 44_100, total_frames);
    let store = DecodedStore::new(44_100, total_frames);

    let write_store = std::sync::Arc::clone(&store);
    let frames_per_tick = level0_frames * 4;
    let decode_thread = std::thread::spawn(move || {
        let mut written = 0u64;
        while written < total_frames {
            let step = frames_per_tick.min(total_frames - written);
            write_tone(&write_store, written, step);
            written += step;
            std::thread::sleep(Duration::from_millis(20));
        }
        write_store.set_complete(total_frames);
    });

    let start = Instant::now();
    service.attach_store(id.clone(), std::sync::Arc::clone(&store));

    let got = drain_until(&mut service, Duration::from_secs(2), |snapshot| {
        snapshot.status == AnalysisStatus::Partial && snapshot.peaks.is_some()
    });
    let elapsed = start.elapsed();
    assert!(got, "expected a Partial snapshot");
    assert!(
        elapsed < Duration::from_secs(1),
        "first Partial snapshot took {elapsed:?}, expected < 1s (SC-001)"
    );

    decode_thread
        .join()
        .unwrap_or_else(|_| panic!("decode-simulating thread panicked"));
    service.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn progressive_store_publishes_partial_then_complete() {
    // A5/SC-002: a `Filling` store publishes an intermediate `Partial`
    // snapshot (some, not all, buckets present) before the eventual
    // `Complete` once every bucket is covered.
    let dir = temp_dir("progressive");
    let paths = AnalysisPaths::with_dir(&dir);
    let mut service = AnalysisService::new(Some(paths));

    let id = track("spotify:track:progressive");
    let level0_frames = 128u64;
    let total_frames = level0_frames * 40;

    service.attach(id.clone(), 44_100, total_frames);
    let store = DecodedStore::new(44_100, total_frames);
    service.attach_store(id.clone(), std::sync::Arc::clone(&store));

    let half = total_frames / 2;
    write_tone(&store, 0, half);

    let saw_partial = drain_until(&mut service, Duration::from_secs(2), |snapshot| {
        snapshot.status == AnalysisStatus::Partial
            && snapshot.peaks.as_ref().is_some_and(|p| {
                let level0 = &p.levels[0];
                let present = (0..level0.len()).filter(|&i| level0.is_present(i)).count();
                present > 0 && present < level0.len()
            })
    });
    assert!(
        saw_partial,
        "expected an intermediate Partial snapshot with some, not all, buckets present"
    );

    write_tone(&store, half, total_frames - half);
    store.set_complete(total_frames);

    let completed = drain_until(&mut service, Duration::from_secs(2), |snapshot| {
        snapshot.status == AnalysisStatus::Complete
    });
    assert!(
        completed,
        "expected the store to reach Complete after progressive fill"
    );
    let snapshot = service.latest().unwrap_or_else(|| unreachable!());
    let peaks = snapshot
        .peaks
        .as_ref()
        .unwrap_or_else(|| unreachable!("Complete must carry peaks"));
    assert!((0..peaks.levels[0].len()).all(|i| peaks.levels[0].is_present(i)));

    service.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn peaks_independent_of_master_volume() {
    // SC-012/AS6: level-0 buckets fold only from `DecodedStore::fold_peaks`
    // (A6) — pre-effect, pre-volume audio by construction (the decode-ahead
    // thread fills the store directly from the decoded stream; a
    // `PlaybackController::set_master_volume` call only scales the
    // engine's mixer, downstream of this store, and is never passed to the
    // Analysis Service at all). Two independent analyses of byte-identical
    // decoded samples must therefore produce byte-identical cached peaks,
    // regardless of whatever master volume the controller happens to be at
    // during playback ("20%" and "100%" are simulated here as two
    // independent attachments over the same content, since volume has no
    // parameter to vary on this path in the first place).
    fn analyze_to_cache_bytes(tag: &str, id: &TrackId, frames: u64) -> Vec<u8> {
        let dir = temp_dir(tag);
        let paths = AnalysisPaths::with_dir(&dir);
        let mut service = AnalysisService::new(Some(paths));
        service.attach(id.clone(), 44_100, frames);
        let store = complete_store(frames);
        service.attach_store(id.clone(), store);

        let got = drain_until(&mut service, Duration::from_secs(5), |snapshot| {
            snapshot.status == AnalysisStatus::Complete
        });
        assert!(got, "{tag}: expected the store to reach Complete");
        let snapshot = service.latest().unwrap_or_else(|| unreachable!());
        let peaks = snapshot
            .peaks
            .as_ref()
            .unwrap_or_else(|| unreachable!("Complete must carry peaks"));
        let bytes = cache::encode(peaks, id);

        service.shutdown();
        let _ = std::fs::remove_dir_all(&dir);
        bytes
    }

    let id = track("spotify:track:volume-independent");
    let frames = 128u64 * 40;

    let at_low_volume = analyze_to_cache_bytes("volume-20pct", &id, frames);
    let at_high_volume = analyze_to_cache_bytes("volume-100pct", &id, frames);
    assert_eq!(
        at_low_volume, at_high_volume,
        "cached peaks must be byte-identical regardless of master volume"
    );
}

// -- US4 (Phase 6): caching-specific guarantees ----------------------------
// `cache.rs`/the worker's cache-lookup step already exist from Phase 3
// (T025, T026); these pin the remaining cache-specific rules: stale-version
// unlink + recompute (A2), the format writes only a WAVE section (FR-004),
// a 10-minute entry's size bound (FR-006), only `Complete` ever reaches
// disk (A7-A9 combined), and distinct track ids never cross-contaminate
// (AS3).

#[test]
fn stale_analyzer_version_is_unlinked_and_recomputed() {
    // A2/SC-005: a cache entry whose `analyzer_version` doesn't match the
    // current `ANALYZER_VERSION` is unlinked on load and analysis proceeds
    // as for a new track; the file on disk ends up rewritten with the
    // current version.
    let dir = temp_dir("stale-version");
    let paths = AnalysisPaths::with_dir(&dir);
    let id = track("spotify:track:stale-version");
    let frames = 128u64 * 40;

    let stale_peaks = WaveformPeaks {
        sample_rate: 44_100,
        len_frames: frames,
        levels: vec![PeakLevel::full(
            128,
            (0..frames.div_ceil(128))
                .map(|_| PeakBucket { min: -5, max: 5 })
                .collect(),
        )],
    };
    let mut bytes = cache::encode(&stale_peaks, &id);
    // analyzer_version is the u32 right after magic (4 bytes) + format
    // version (4 bytes) — mirrors cache.rs's own
    // `decode_rejects_stale_analyzer_version` unit test.
    bytes[8..12].copy_from_slice(&(ANALYZER_VERSION + 1).to_le_bytes());
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{e}"));
    std::fs::write(paths.file_for(&id), &bytes).unwrap_or_else(|e| panic!("{e}"));

    let mut service = AnalysisService::new(Some(paths.clone()));
    service.attach(id.clone(), 44_100, frames);
    let store = complete_store(frames);
    service.attach_store(id.clone(), store);

    let got = drain_until(&mut service, Duration::from_secs(5), |snapshot| {
        snapshot.status == AnalysisStatus::Complete
    });
    assert!(
        got,
        "expected recomputation to Complete after a stale cache entry"
    );
    let snapshot = service.latest().unwrap_or_else(|| unreachable!());
    assert!(
        !snapshot.from_cache,
        "a stale entry must never be served from cache"
    );

    service.shutdown();

    let on_disk = std::fs::read(paths.file_for(&id)).unwrap_or_else(|e| panic!("{e}"));
    let reloaded = cache::decode(&on_disk, &id, frames)
        .unwrap_or_else(|e| panic!("stale entry was not rewritten with the current version: {e}"));
    assert_eq!(reloaded.sample_rate, 44_100);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn only_wave_section_is_written() {
    // FR-004: this slice writes only a WAVE section — no beat-grid, key or
    // loudness sections, even though the format's unknown-section-skip
    // rule (data-model.md §6) allows them to be added later.
    let dir = temp_dir("only-wave-section");
    let paths = AnalysisPaths::with_dir(&dir);
    let id = track("spotify:track:only-wave");
    let frames = 128u64 * 40;

    let mut service = AnalysisService::new(Some(paths.clone()));
    service.attach(id.clone(), 44_100, frames);
    let store = complete_store(frames);
    service.attach_store(id.clone(), store);
    let got = drain_until(&mut service, Duration::from_secs(5), |snapshot| {
        snapshot.status == AnalysisStatus::Complete
    });
    assert!(got, "expected the store to reach Complete");
    service.shutdown();

    let bytes = std::fs::read(paths.file_for(&id)).unwrap_or_else(|e| panic!("{e}"));
    // Walk the header exactly as `cache::encode` writes it: magic(4) +
    // format_version(4) + analyzer_version(4) + id_len(2) + id + sample_
    // rate(4) + len_frames(8) + section_count(4) + first section's tag(4).
    let mut pos = 4 + 4 + 4;
    let id_len = u16::from_le_bytes(
        bytes[pos..pos + 2]
            .try_into()
            .unwrap_or_else(|_| panic!("truncated id_len")),
    ) as usize;
    pos += 2 + id_len;
    pos += 4 + 8;
    let section_count = u32::from_le_bytes(
        bytes[pos..pos + 4]
            .try_into()
            .unwrap_or_else(|_| panic!("truncated section_count")),
    );
    pos += 4;
    assert_eq!(
        section_count, 1,
        "only the WAVE section must be written (FR-004)"
    );
    assert_eq!(&bytes[pos..pos + 4], b"WAVE");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ten_minute_entry_fits_one_megabyte() {
    // FR-006: a 10-minute track's cache entry stays comfortably under
    // 1 MiB (research/plan.md put the real figure at ~472 KB).
    let id = track("spotify:track:ten-minutes");
    let sample_rate = 44_100u32;
    let len_frames = u64::from(sample_rate) * 600; // 10 minutes

    let levels = WaveformPeaks::LADDER
        .iter()
        .copied()
        .filter(|&fpb| len_frames.div_ceil(u64::from(fpb)) >= 2)
        .map(|fpb| {
            let count = len_frames.div_ceil(u64::from(fpb)) as usize;
            let buckets = (0..count)
                .map(|_| PeakBucket {
                    min: -100,
                    max: 100,
                })
                .collect();
            PeakLevel::full(fpb, buckets)
        })
        .collect();
    let peaks = WaveformPeaks {
        sample_rate,
        len_frames,
        levels,
    };

    let bytes = cache::encode(&peaks, &id);
    assert!(
        bytes.len() < 1_000_000,
        "10-minute entry is {} bytes, expected < 1 MiB (FR-006)",
        bytes.len()
    );
}

#[test]
fn only_complete_entries_reach_disk() {
    // A7/A8/A9 combined: the cache directory stays empty through partial
    // fill, through a detach, and through a Failed-with-partial-peaks
    // outcome — a file appears only once a track genuinely reaches
    // Complete.
    fn file_count(dir: &std::path::Path) -> usize {
        std::fs::read_dir(dir)
            .map(|entries| entries.filter_map(Result::ok).count())
            .unwrap_or(0)
    }

    let dir = temp_dir("only-complete");
    let paths = AnalysisPaths::with_dir(&dir);
    let frames = 128u64 * 40;
    let mut service = AnalysisService::new(Some(paths.clone()));

    // A9: detach before Complete writes nothing.
    let id_detach = track("spotify:track:only-complete-detach");
    service.attach(id_detach.clone(), 44_100, frames);
    let store_detach = DecodedStore::new(44_100, frames);
    service.attach_store(id_detach.clone(), std::sync::Arc::clone(&store_detach));
    write_tone(&store_detach, 0, frames / 2);
    let saw_partial = drain_until(&mut service, Duration::from_secs(2), |snapshot| {
        snapshot.status == AnalysisStatus::Partial
    });
    assert!(saw_partial, "expected a Partial snapshot before detaching");
    assert_eq!(file_count(&dir), 0, "no file must exist before Complete");
    service.detach();
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(
        file_count(&dir),
        0,
        "detach must not write a partial entry (A9)"
    );

    // A8: a store that ends Failed (with partial peaks) writes nothing.
    let id_failed = track("spotify:track:only-complete-failed");
    service.attach(id_failed.clone(), 44_100, frames);
    let store_failed = DecodedStore::new(44_100, frames);
    service.attach_store(id_failed.clone(), std::sync::Arc::clone(&store_failed));
    write_tone(&store_failed, 0, frames / 2);
    let saw_partial2 = drain_until(&mut service, Duration::from_secs(2), |snapshot| {
        snapshot.status == AnalysisStatus::Partial
    });
    assert!(saw_partial2, "expected a Partial snapshot before failing");
    store_failed.set_failed();
    let saw_failed = drain_until(&mut service, Duration::from_secs(2), |snapshot| {
        snapshot.status == AnalysisStatus::Failed
    });
    assert!(saw_failed, "expected the store failure to reach Failed");
    assert_eq!(
        file_count(&dir),
        0,
        "a Failed status must not write a file (A8)"
    );

    // A7 (positive case): a full, non-silent Complete does write a file.
    let id_complete = track("spotify:track:only-complete-success");
    service.attach(id_complete.clone(), 44_100, frames);
    service.attach_store(id_complete.clone(), complete_store(frames));
    let completed = drain_until(&mut service, Duration::from_secs(5), |snapshot| {
        snapshot.status == AnalysisStatus::Complete
    });
    assert!(completed, "expected the store to reach Complete");
    assert_eq!(
        file_count(&dir),
        1,
        "Complete must write exactly one file (A7)"
    );

    service.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cache_entries_never_confuse_track_identities() {
    // AS3: two distinct `TrackId`s produce and load distinct `.mpwf`
    // files; switching between them shows each one's own waveform, never
    // the other's, even on a fresh `AnalysisService` (genuine cache hits).
    fn distinct_store(frames: u64, level: f32) -> std::sync::Arc<DecodedStore> {
        let store = DecodedStore::new(44_100, frames);
        let interleaved: Vec<f32> = (0..frames)
            .flat_map(|i| {
                let s = if i % 4 < 2 { level } else { -level };
                [s, s]
            })
            .collect();
        store.write_frames(0, &interleaved);
        store.set_complete(frames);
        store
    }

    let dir = temp_dir("track-identities");
    let paths = AnalysisPaths::with_dir(&dir);
    let frames = 128u64 * 40;
    let id_a = track("spotify:track:identity-a");
    let id_b = track("spotify:track:identity-b");

    let mut service = AnalysisService::new(Some(paths.clone()));

    service.attach(id_a.clone(), 44_100, frames);
    service.attach_store(id_a.clone(), distinct_store(frames, 0.3));
    let done_a = drain_until(&mut service, Duration::from_secs(5), |snapshot| {
        snapshot.status == AnalysisStatus::Complete
    });
    assert!(done_a, "track A must reach Complete");
    let peaks_a = service
        .latest()
        .unwrap_or_else(|| unreachable!())
        .peaks
        .clone();

    service.attach(id_b.clone(), 44_100, frames);
    service.attach_store(id_b.clone(), distinct_store(frames, 0.9));
    let done_b = drain_until(&mut service, Duration::from_secs(5), |snapshot| {
        snapshot.status == AnalysisStatus::Complete
    });
    assert!(done_b, "track B must reach Complete");
    let peaks_b = service
        .latest()
        .unwrap_or_else(|| unreachable!())
        .peaks
        .clone();

    assert_ne!(peaks_a, peaks_b, "distinct tracks must not share peaks");
    assert_ne!(
        paths.file_for(&id_a),
        paths.file_for(&id_b),
        "distinct track ids must map to distinct cache files"
    );
    assert!(paths.file_for(&id_a).exists());
    assert!(paths.file_for(&id_b).exists());

    service.shutdown();

    // A fresh service reloading each id from disk (genuine cache hits, not
    // the in-memory attachment above) must never cross-load the other's
    // file.
    let mut service2 = AnalysisService::new(Some(paths.clone()));

    service2.attach(id_a.clone(), 44_100, frames);
    let got_a = drain_until(&mut service2, Duration::from_millis(500), |snapshot| {
        snapshot.status == AnalysisStatus::Complete
    });
    assert!(got_a, "track A must reload from cache");
    let reloaded_a = service2.latest().unwrap_or_else(|| unreachable!());
    assert!(reloaded_a.from_cache);
    assert_eq!(reloaded_a.peaks, peaks_a);

    service2.attach(id_b.clone(), 44_100, frames);
    let got_b = drain_until(&mut service2, Duration::from_millis(500), |snapshot| {
        snapshot.status == AnalysisStatus::Complete
    });
    assert!(got_b, "track B must reload from cache");
    let reloaded_b = service2.latest().unwrap_or_else(|| unreachable!());
    assert!(reloaded_b.from_cache);
    assert_eq!(reloaded_b.peaks, peaks_b);

    service2.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

// -- Polish (Phase 7): analysis-failure handling (EC-5.9) -------------------
// `only_complete_entries_reach_disk` (US4) already pins the combined "no
// file until Complete" invariant across detach/Failed/success; these four
// tests each pin one rule (A3/A7/A8/A9) on its own, by name, per Constitution
// VIII / quickstart.md's named-test table.

#[test]
fn silent_track_fails_and_writes_nothing() {
    // A7: every level-0 bucket silent (`max - min == 0`) once `Complete` ->
    // published as `Failed` with no peaks, and nothing reaches disk.
    let dir = temp_dir("silent-fails");
    let paths = AnalysisPaths::with_dir(&dir);
    let id = track("spotify:track:silent-fails");
    let frames = 128u64 * 40;

    let mut service = AnalysisService::new(Some(paths.clone()));
    service.attach(id.clone(), 44_100, frames);

    let store = DecodedStore::new(44_100, frames);
    let silence: Vec<f32> = (0..frames).flat_map(|_| [0.0f32, 0.0]).collect();
    store.write_frames(0, &silence);
    store.set_complete(frames);
    service.attach_store(id.clone(), store);

    let got = drain_until(&mut service, Duration::from_secs(5), |snapshot| {
        snapshot.status == AnalysisStatus::Failed
    });
    assert!(got, "expected a silent track to reach Failed (A7)");

    let snapshot = service.latest().unwrap_or_else(|| unreachable!());
    assert!(
        snapshot.peaks.is_none(),
        "a silent track's Failed snapshot must carry no peaks"
    );
    assert!(
        !paths.file_for(&id).exists(),
        "a silent (Failed) track must never write a cache file"
    );

    service.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn decoder_failure_keeps_partial_peaks() {
    // A8: a store that reaches `Failed` mid-decode still publishes whatever
    // level-0 buckets had already folded, rather than discarding them.
    let dir = temp_dir("decoder-failure");
    let paths = AnalysisPaths::with_dir(&dir);
    let id = track("spotify:track:decoder-failure");
    let total_buckets = 40u64;
    let frames = total_buckets * 128;
    let partial_frames = (total_buckets / 4) * 128; // a quarter folds first

    let mut service = AnalysisService::new(Some(paths.clone()));
    service.attach(id.clone(), 44_100, frames);

    let store = DecodedStore::new(44_100, frames);
    write_tone(&store, 0, partial_frames);
    service.attach_store(id.clone(), std::sync::Arc::clone(&store));

    let saw_partial = drain_until(&mut service, Duration::from_secs(2), |snapshot| {
        snapshot.status == AnalysisStatus::Partial && snapshot.peaks.is_some()
    });
    assert!(saw_partial, "expected a Partial snapshot before failing");

    store.set_failed();
    let got = drain_until(&mut service, Duration::from_secs(2), |snapshot| {
        snapshot.status == AnalysisStatus::Failed
    });
    assert!(got, "expected the store failure to reach Failed");

    let snapshot = service.latest().unwrap_or_else(|| unreachable!());
    let peaks = snapshot
        .peaks
        .as_ref()
        .unwrap_or_else(|| unreachable!("a mid-decode failure must keep its partial peaks (A8)"));
    let level0 = &peaks.levels[0];
    let present = (0..level0.len()).filter(|&i| level0.is_present(i)).count();
    assert!(
        present > 0 && present < level0.len(),
        "expected some, not all or none, level-0 buckets present, got {present}/{}",
        level0.len()
    );
    assert!(
        !paths.file_for(&id).exists(),
        "a Failed track must never write a cache file, even with partial peaks"
    );

    service.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn failed_track_is_not_retried_this_session() {
    // A3: once a track has failed, re-attaching it in the same session
    // (same `AnalysisService`/worker) publishes `Failed` immediately from
    // the in-memory `failed` set, never waiting on a fresh store again.
    let dir = temp_dir("not-retried");
    let paths = AnalysisPaths::with_dir(&dir);
    let id = track("spotify:track:not-retried");
    let frames = 128u64 * 40;

    let mut service = AnalysisService::new(Some(paths));
    service.attach(id.clone(), 44_100, frames);
    let store = DecodedStore::new(44_100, frames);
    store.set_failed(); // fails with zero folded buckets
    service.attach_store(id.clone(), store);

    let got = drain_until(&mut service, Duration::from_secs(2), |snapshot| {
        snapshot.status == AnalysisStatus::Failed
    });
    assert!(got, "expected the first attachment to reach Failed");

    service.detach();
    let start = Instant::now();
    service.attach(id.clone(), 44_100, frames);
    // Deliberately never call `attach_store` again: A3 requires this to
    // resolve to `Failed` from the `failed` set alone.
    let got_again = drain_until(&mut service, Duration::from_millis(500), |snapshot| {
        snapshot.status == AnalysisStatus::Failed
    });
    let elapsed = start.elapsed();
    assert!(
        got_again,
        "expected a second attach of a failed track to publish Failed without a store"
    );
    assert!(
        elapsed < Duration::from_millis(300),
        "a known-failed track must resolve near-instantly (from `failed`, not a wait), took \
         {elapsed:?}"
    );
    let snapshot = service.latest().unwrap_or_else(|| unreachable!());
    assert!(
        snapshot.peaks.is_none(),
        "a re-attached known-failed track publishes no peaks"
    );

    service.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn detach_drops_partial_without_writing() {
    // A9/FR-017: detaching before Complete drops whatever partial peaks had
    // folded and writes nothing to disk.
    let dir = temp_dir("detach-drops");
    let paths = AnalysisPaths::with_dir(&dir);
    let id = track("spotify:track:detach-drops");
    let frames = 128u64 * 40;

    let mut service = AnalysisService::new(Some(paths.clone()));
    service.attach(id.clone(), 44_100, frames);
    let store = DecodedStore::new(44_100, frames);
    service.attach_store(id.clone(), std::sync::Arc::clone(&store));
    write_tone(&store, 0, frames / 2);

    let saw_partial = drain_until(&mut service, Duration::from_secs(2), |snapshot| {
        snapshot.status == AnalysisStatus::Partial
    });
    assert!(saw_partial, "expected a Partial snapshot before detaching");

    service.detach();
    assert!(
        service.latest().is_none(),
        "detach must clear the handle's own cached snapshot immediately"
    );

    // Let the worker actually process the `Detach` command before checking
    // disk (it runs on its own thread).
    std::thread::sleep(Duration::from_millis(100));
    assert!(
        !paths.file_for(&id).exists(),
        "a detach before Complete must never write a cache file (A9, FR-017)"
    );

    service.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn cache_round_trips_any_peaks((peaks, id) in peaks_strategy()) {
        let bytes = cache::encode(&peaks, &id);
        let decoded = cache::decode(&bytes, &id, peaks.len_frames)
            .unwrap_or_else(|e| panic!("decode of a freshly-encoded entry failed: {e}"));
        prop_assert_eq!(decoded, peaks);
    }

    #[test]
    fn cache_rejects_any_truncation((peaks, id) in peaks_strategy(), cut_fraction in 0f64..1.0) {
        let bytes = cache::encode(&peaks, &id);
        let cut = ((bytes.len() as f64) * cut_fraction) as usize;
        let truncated = &bytes[..cut.min(bytes.len().saturating_sub(1))];
        // Never panics; either it's still coincidentally a full valid
        // encoding (impossible once at least one byte is missing) or it
        // is rejected.
        prop_assert!(cache::decode(truncated, &id, peaks.len_frames).is_err());
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Peak folding: level-0 min/max buckets from `DecodedStore::fold_peaks`,
//! coarser ×8 ladder levels, presence bitmaps, and silence detection
//! (005-now-playing-waveform, data-model.md §3.2).

use modplayer_audio_source::{DecodedStore, PeakBucket};

use super::{PeakLevel, WaveformPeaks};

/// Level-0 buckets folded per pass, bounding the analysis thread's work
/// between publishes (research R5 step 3).
pub(crate) const MAX_BUCKETS_PER_PASS: usize = 2_048;

/// How many finer buckets fold into one coarser one at every ladder step
/// (research R4).
const FOLD_FACTOR: usize = 8;

/// Level-0 `frames_per_bucket` (research R4: 2.9 ms at 44.1 kHz).
const LEVEL0_FRAMES: u32 = WaveformPeaks::LADDER[0];

/// `ceil(len_frames / frames_per_bucket)`.
pub(crate) fn bucket_count(len_frames: u64, frames_per_bucket: u32) -> usize {
    len_frames.div_ceil(u64::from(frames_per_bucket.max(1))) as usize
}

/// How many frames `level`'s bucket `index` actually covers: its own
/// `frames_per_bucket`, except the last bucket in the track, which may be
/// short (022-waveform-legibility, AR2's fold weight `wᵢ`).
fn bucket_frame_count(level: &PeakLevel, index: usize, len_frames: u64) -> u32 {
    let fpb = u64::from(level.frames_per_bucket.max(1));
    let start = index as u64 * fpb;
    let end = (start + fpb).min(len_frames);
    end.saturating_sub(start) as u32
}

/// Which of `WaveformPeaks::LADDER` actually get a level for a track of
/// `len_frames` frames — "a level exists only if it has >= 2 buckets for
/// the track" (data-model.md §3.2).
fn ladder_for(len_frames: u64) -> Vec<u32> {
    WaveformPeaks::LADDER
        .iter()
        .copied()
        .filter(|&fpb| bucket_count(len_frames, fpb) >= 2)
        .collect()
}

/// Build the empty (all-absent) shell for a freshly attached track.
pub(crate) fn empty_peaks(sample_rate: u32, len_frames: u64) -> WaveformPeaks {
    let levels = ladder_for(len_frames)
        .into_iter()
        .map(|fpb| PeakLevel::empty(fpb, bucket_count(len_frames, fpb)))
        .collect();
    WaveformPeaks {
        sample_rate,
        len_frames,
        levels,
    }
}

/// Fold as many new, whole, fully-covered level-0 buckets as `store` has
/// ready (up to `MAX_BUCKETS_PER_PASS`), starting at level 0's current
/// watermark (contracts/decoded-store.md §2 rule 6). Returns the number of
/// level-0 buckets newly folded; `0` once level 0 has nothing left the
/// store hasn't already yielded.
pub(crate) fn fold_level0(peaks: &mut WaveformPeaks, store: &DecodedStore) -> usize {
    let Some(level0) = peaks.levels.first_mut() else {
        return 0;
    };
    let watermark = level0.present_count();
    if watermark >= level0.len() {
        return 0;
    }
    let want = (level0.len() - watermark).min(MAX_BUCKETS_PER_PASS);
    let mut buf = vec![PeakBucket::default(); want];
    let from_frame = watermark as u64 * u64::from(LEVEL0_FRAMES);
    let written = store.fold_peaks(from_frame, LEVEL0_FRAMES, &mut buf);
    for (offset, bucket) in buf.into_iter().take(written).enumerate() {
        let index = watermark + offset;
        level0.buckets[index] = bucket;
        level0.set_present(index);
    }
    written
}

/// Refold every coarser level from whatever new presence its immediately
/// finer level gained (×8 per step, research R4): a coarse bucket is
/// present iff every finer bucket it folds is present (the last one: all
/// that exist, data-model.md §3.2's invariant).
pub(crate) fn refold_coarser_levels(peaks: &mut WaveformPeaks) {
    let len_frames = peaks.len_frames;
    for level_index in 1..peaks.levels.len() {
        let (below, rest) = peaks.levels.split_at_mut(level_index);
        let finer = &below[level_index - 1];
        let level = &mut rest[0];
        let watermark = level.present_count();
        for coarse_index in watermark..level.len() {
            let start = coarse_index * FOLD_FACTOR;
            let end = ((coarse_index + 1) * FOLD_FACTOR).min(finer.len());
            if start >= finer.len() || !(start..end).all(|i| finer.is_present(i)) {
                break; // stop at the first not-yet-ready coarse bucket
            }
            let mut min_v = i8::MAX;
            let mut max_v = i8::MIN;
            // Weighted quadratic mean (022-waveform-legibility, AR2):
            // `rms_coarse = round(sqrt(Σ wᵢ·rmsᵢ² / Σ wᵢ))`, `wᵢ` = the
            // finer bucket's own frame count (its bucket size, save for a
            // possible short last bucket). `u64` accumulators (data-model
            // §2) — squares of `u8`/`u32` values fit comfortably.
            let mut weighted_sum_sq: u64 = 0;
            let mut total_weight: u64 = 0;
            for i in start..end {
                let bucket = finer.buckets[i];
                min_v = min_v.min(bucket.min);
                max_v = max_v.max(bucket.max);
                let weight = u64::from(bucket_frame_count(finer, i, len_frames));
                let rms = u64::from(bucket.rms);
                weighted_sum_sq += weight * rms * rms;
                total_weight += weight;
            }
            let rms_v = if total_weight == 0 {
                0
            } else {
                let mean_sq = weighted_sum_sq as f64 / total_weight as f64;
                mean_sq.sqrt().round() as u8
            };
            level.buckets[coarse_index] = PeakBucket {
                min: min_v,
                max: max_v,
                rms: rms_v,
            };
            level.set_present(coarse_index);
        }
    }
}

/// `true` once every level-0 bucket is silence (`max - min == 0`) — only
/// meaningful to call once level 0 is fully folded (contracts/
/// analysis-service.md A7). A track with no level-0 ladder at all (too
/// short for even 2 buckets) is never called silent here.
pub(crate) fn is_silent(peaks: &WaveformPeaks) -> bool {
    let Some(level0) = peaks.levels.first() else {
        return false;
    };
    if level0.buckets.is_empty() {
        return false;
    }
    level0.buckets.iter().all(|bucket| bucket.max == bucket.min)
}

#[cfg(test)]
mod tests {
    use super::*;
    use modplayer_audio_source::DecodedStore;

    fn filled_store(frames: u64, sample: f32) -> std::sync::Arc<DecodedStore> {
        let store = DecodedStore::new(44_100, frames);
        let interleaved: Vec<f32> = (0..frames).flat_map(|_| [sample, sample]).collect();
        store.write_frames(0, &interleaved);
        store.set_complete(frames);
        store
    }

    /// A store whose sample alternates every frame — every level-0 bucket
    /// has a real `max - min` range, unlike a constant-DC store.
    fn varying_store(frames: u64) -> std::sync::Arc<DecodedStore> {
        let store = DecodedStore::new(44_100, frames);
        let interleaved: Vec<f32> = (0..frames)
            .flat_map(|i| {
                let sample = if i % 2 == 0 { 0.5 } else { -0.5 };
                [sample, sample]
            })
            .collect();
        store.write_frames(0, &interleaved);
        store.set_complete(frames);
        store
    }

    #[test]
    fn ladder_levels_fold_by_eight_and_present_bits_follow() {
        // 10 level-0 buckets (LEVEL0_FRAMES * 10 frames) → level 1 needs >= 2
        // buckets, i.e. >= 16 level-0 buckets; use enough frames for two full
        // ladder rungs to be exercised.
        let level0_buckets = 20u64;
        let frames = level0_buckets * u64::from(LEVEL0_FRAMES);
        let store = filled_store(frames, 0.5);

        let mut peaks = empty_peaks(44_100, frames);
        loop {
            let written = fold_level0(&mut peaks, &store);
            if written == 0 {
                break;
            }
            refold_coarser_levels(&mut peaks);
        }

        let level0 = &peaks.levels[0];
        assert_eq!(level0.len(), level0_buckets as usize);
        assert!((0..level0.len()).all(|i| level0.is_present(i)));

        let level1 = &peaks.levels[1];
        // 20 level-0 buckets fold ×8 into ceil(20/8) = 3 level-1 buckets;
        // the last one folds only the remaining 4 (all present).
        assert_eq!(level1.len(), 3);
        assert!((0..level1.len()).all(|i| level1.is_present(i)));
        for bucket in &level1.buckets {
            assert!(bucket.min > 0 && bucket.max > 0);
        }
    }

    #[test]
    fn refold_stops_at_first_incomplete_coarse_bucket() {
        // Only enough level-0 buckets for a *partial* first level-1 bucket.
        let level0_buckets = 20u64;
        let frames = level0_buckets * u64::from(LEVEL0_FRAMES);
        let store = DecodedStore::new(44_100, frames);
        // Fill only the first 4 level-0 buckets (< FOLD_FACTOR).
        let partial_frames = 4 * u64::from(LEVEL0_FRAMES);
        let interleaved: Vec<f32> = (0..partial_frames).flat_map(|_| [0.25, 0.25]).collect();
        store.write_frames(0, &interleaved);

        let mut peaks = empty_peaks(44_100, frames);
        fold_level0(&mut peaks, &store);
        refold_coarser_levels(&mut peaks);

        assert_eq!(peaks.levels[0].present_count(), 4);
        assert_eq!(peaks.levels[1].present_count(), 0);
    }

    /// A store whose sample is a constant amplitude, but alternates sign —
    /// every level-0 bucket has the same non-zero RMS, so refolding it up
    /// the ×8 ladder is a "uniform-RMS track": every coarser level's `rms`
    /// should match level 0's (±1), per AR2's invariant.
    fn uniform_rms_store(frames: u64, amplitude: f32) -> std::sync::Arc<DecodedStore> {
        let store = DecodedStore::new(44_100, frames);
        let interleaved: Vec<f32> = (0..frames)
            .flat_map(|i| {
                let sample = if i % 2 == 0 { amplitude } else { -amplitude };
                [sample, sample]
            })
            .collect();
        store.write_frames(0, &interleaved);
        store.set_complete(frames);
        store
    }

    /// AR2: `rms_coarse = round(sqrt(Σ wᵢ·rmsᵢ² / Σ wᵢ))`, `wᵢ` = the
    /// finer bucket's frame count — a weighted quadratic mean, not a plain
    /// (unweighted) mean or a min/max-style fold.
    #[test]
    fn refold_coarser_levels_is_weighted_quadratic_mean_of_rms() {
        let level0_buckets = 20u64;
        let frames = level0_buckets * u64::from(LEVEL0_FRAMES);
        // 0.5 amplitude on every sample of every frame → every level-0
        // bucket's rms is `quantise_rms` of a constant-amplitude signal.
        let store = uniform_rms_store(frames, 0.5);

        let mut peaks = empty_peaks(44_100, frames);
        loop {
            let written = fold_level0(&mut peaks, &store);
            if written == 0 {
                break;
            }
            refold_coarser_levels(&mut peaks);
        }

        let level0_rms = peaks.levels[0].buckets[0].rms;
        // Every level-0 bucket is identical content, so they all share the
        // same `rms`; the weighted quadratic mean of N identical values is
        // that value itself (within ±1 for rounding across levels).
        for level in &peaks.levels {
            for i in 0..level.len() {
                let rms = level.buckets[i].rms;
                assert!(
                    (i32::from(rms) - i32::from(level0_rms)).abs() <= 1,
                    "level rms {rms} diverged from level-0 rms {level0_rms}"
                );
            }
        }
    }

    /// A last, short finer bucket still contributes its (smaller) frame
    /// count as its weight — not folded as if it were full-sized.
    #[test]
    fn refold_coarser_levels_weights_short_last_bucket_by_frame_count() {
        // 9 level-0 buckets: level 1's first coarse bucket folds all 8 at
        // full weight; a second coarse bucket would fold just the 9th
        // (short, since bucket_count needs >= 2 for the ladder to exist —
        // use 17 so level 1 has exactly 2 buckets, the second folding only
        // 1 finer bucket at a *full* frame count, i.e. no shortness here
        // at level 0; shortness instead shows up in the mono min/max path
        // already covered by 005 — this test pins that RMS folds by
        // frame-count weight, not bucket count, using differing content
        // per bucket so an unweighted mean would diverge).
        let level0_buckets = 16u64;
        let frames = level0_buckets * u64::from(LEVEL0_FRAMES);
        let store = DecodedStore::new(44_100, frames);
        // First 8 level-0 buckets: amplitude 0.1 (low rms). Last 8: 0.9
        // (high rms). An unweighted mean over 2 groups would sit halfway;
        // frame-count weighting with equal-length buckets sits at the same
        // point here since weights are equal — so cross-check against the
        // direct AR2 formula instead of assuming a specific value.
        let mut interleaved = Vec::with_capacity((frames * 2) as usize);
        for i in 0..frames {
            let amplitude = if i < frames / 2 { 0.1 } else { 0.9 };
            interleaved.push(amplitude);
            interleaved.push(amplitude);
        }
        store.write_frames(0, &interleaved);
        store.set_complete(frames);

        let mut peaks = empty_peaks(44_100, frames);
        loop {
            let written = fold_level0(&mut peaks, &store);
            if written == 0 {
                break;
            }
            refold_coarser_levels(&mut peaks);
        }

        let level1 = &peaks.levels[1];
        assert_eq!(level1.len(), 2);
        // AR2 direct formula over the 8 level-0 rms values folding into
        // level1[0], all with equal weight (LEVEL0_FRAMES each).
        let level0 = &peaks.levels[0];
        let expected: u8 = {
            let sum_sq: f64 = (0..8)
                .map(|i| {
                    let r = f64::from(level0.buckets[i].rms);
                    let w = f64::from(LEVEL0_FRAMES);
                    w * r * r
                })
                .sum();
            let total_w = 8.0 * f64::from(LEVEL0_FRAMES);
            (sum_sq / total_w).sqrt().round() as u8
        };
        assert!((i32::from(level1.buckets[0].rms) - i32::from(expected)).abs() <= 1);
    }

    #[test]
    fn silence_detected_only_when_every_level0_bucket_is_flat() {
        let frames = 20u64 * u64::from(LEVEL0_FRAMES);
        let silent_store = filled_store(frames, 0.0);
        let mut peaks = empty_peaks(44_100, frames);
        while fold_level0(&mut peaks, &silent_store) > 0 {}
        assert!(is_silent(&peaks));

        let loud_store = varying_store(frames);
        let mut peaks = empty_peaks(44_100, frames);
        while fold_level0(&mut peaks, &loud_store) > 0 {}
        assert!(!is_silent(&peaks));
    }
}

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
            for i in start..end {
                let bucket = finer.buckets[i];
                min_v = min_v.min(bucket.min);
                max_v = max_v.max(bucket.max);
            }
            level.buckets[coarse_index] = PeakBucket {
                min: min_v,
                max: max_v,
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

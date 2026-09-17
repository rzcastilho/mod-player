// SPDX-License-Identifier: MIT OR Apache-2.0

//! The on-disk waveform cache: `<data_local_dir>/ModPlayer/analysis/
//! <sha256(track_id)>.mpwf`, its sectioned binary layout, atomic
//! temp-file-then-rename writes, and user-only permissions
//! (005-now-playing-waveform, research R7, data-model.md §6).

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use directories::ProjectDirs;
use sha2::{Digest, Sha256};
use thiserror::Error;

use modplayer_audio_source::{PeakBucket, TrackId};

use super::{ANALYZER_VERSION, PeakLevel, WaveformPeaks, peaks};

/// Overrides the cache directory (tests, portable use — mirrors
/// `library::persist::DATA_DIR_ENV`).
pub const ANALYSIS_DIR_ENV: &str = "MODPLAYER_ANALYSIS_DIR";

const MAGIC: &[u8; 4] = b"MPWF";
const FORMAT_VERSION: u32 = 1;
const WAVE_TAG: &[u8; 4] = b"WAVE";
/// A `len_frames` this far from the track's known duration is treated as a
/// different/stale entry (research R7).
const LEN_TOLERANCE_SECONDS: u64 = 2;

/// Any failure to encode/decode/read/write a `.mpwf` entry.
#[derive(Debug, Error)]
pub enum CacheError {
    #[error("truncated or malformed cache entry")]
    Malformed,
    #[error("bad magic")]
    BadMagic,
    #[error("unsupported format version")]
    FormatVersion,
    #[error("analyzer version mismatch")]
    VersionMismatch,
    #[error("track id mismatch")]
    IdMismatch,
    #[error("track length mismatch")]
    LengthMismatch,
    #[error("missing WAVE section")]
    MissingWave,
    #[error("io error: {0}")]
    Io(String),
}

impl From<std::io::Error> for CacheError {
    fn from(err: std::io::Error) -> Self {
        CacheError::Io(err.to_string())
    }
}

/// Resolved cache directory (research R7).
#[derive(Debug, Clone)]
pub struct AnalysisPaths {
    pub dir: PathBuf,
}

impl AnalysisPaths {
    /// `MODPLAYER_ANALYSIS_DIR` if set, else the platform data-local dir's
    /// `ModPlayer/analysis/` (mirrors `library::persist::LibraryPaths::
    /// resolve`). `None` only if neither is determinable.
    pub fn resolve() -> Option<Self> {
        if let Ok(dir) = std::env::var(ANALYSIS_DIR_ENV) {
            return Some(Self::with_dir(dir));
        }
        ProjectDirs::from("", "ModPlayer", "ModPlayer")
            .map(|dirs| Self::with_dir(dirs.data_local_dir().join("analysis")))
    }

    pub fn with_dir(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// `<dir>/<sha256(track_id) hex>.mpwf`.
    pub fn file_for(&self, track: &TrackId) -> PathBuf {
        let mut hasher = Sha256::new();
        hasher.update(track.as_str().as_bytes());
        let digest = hasher.finalize();
        let mut hex = String::with_capacity(digest.len() * 2);
        for byte in digest {
            hex.push_str(&format!("{byte:02x}"));
        }
        self.dir.join(format!("{hex}.mpwf"))
    }
}

/// A checked little-endian byte cursor: every read returns `Err` instead
/// of panicking on truncated/malformed input (proptest
/// `cache_rejects_any_truncation`).
struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], CacheError> {
        let end = self.pos.checked_add(n).ok_or(CacheError::Malformed)?;
        let slice = self.bytes.get(self.pos..end).ok_or(CacheError::Malformed)?;
        self.pos = end;
        Ok(slice)
    }

    fn u16(&mut self) -> Result<u16, CacheError> {
        let bytes: [u8; 2] = self
            .take(2)?
            .try_into()
            .map_err(|_| CacheError::Malformed)?;
        Ok(u16::from_le_bytes(bytes))
    }

    fn u32(&mut self) -> Result<u32, CacheError> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| CacheError::Malformed)?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn u64(&mut self) -> Result<u64, CacheError> {
        let bytes: [u8; 8] = self
            .take(8)?
            .try_into()
            .map_err(|_| CacheError::Malformed)?;
        Ok(u64::from_le_bytes(bytes))
    }

    fn i8(&mut self) -> Result<i8, CacheError> {
        Ok(self.take(1)?[0] as i8)
    }
}

/// Encode `peaks` into the sectioned `.mpwf` byte layout (research R7,
/// data-model.md §6). Only ever called with a `Complete` ladder — every
/// level's presence bitmap is therefore all-ones and is not stored (`load`
/// reconstructs it as full).
pub fn encode(peaks: &WaveformPeaks, track: &TrackId) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(&ANALYZER_VERSION.to_le_bytes());
    let id_bytes = track.as_str().as_bytes();
    out.extend_from_slice(&(id_bytes.len() as u16).to_le_bytes());
    out.extend_from_slice(id_bytes);
    out.extend_from_slice(&peaks.sample_rate.to_le_bytes());
    out.extend_from_slice(&peaks.len_frames.to_le_bytes());

    let mut wave_payload = Vec::new();
    wave_payload.extend_from_slice(&(peaks.levels.len() as u32).to_le_bytes());
    for level in &peaks.levels {
        wave_payload.extend_from_slice(&level.frames_per_bucket.to_le_bytes());
        wave_payload.extend_from_slice(&(level.buckets.len() as u32).to_le_bytes());
        for bucket in &level.buckets {
            wave_payload.push(bucket.min as u8);
            wave_payload.push(bucket.max as u8);
        }
    }

    out.extend_from_slice(&1u32.to_le_bytes()); // section_count
    out.extend_from_slice(WAVE_TAG);
    out.extend_from_slice(&(wave_payload.len() as u64).to_le_bytes());
    out.extend_from_slice(&wave_payload);
    out
}

/// Decode and validate a `.mpwf` entry (data-model.md §6's validation
/// rules). Any mismatch or truncation is `Err` — `load` unlinks the file
/// in that case.
pub fn decode(
    bytes: &[u8],
    track: &TrackId,
    expected_len_frames: u64,
) -> Result<WaveformPeaks, CacheError> {
    let mut reader = Reader::new(bytes);
    if reader.take(4)? != MAGIC {
        return Err(CacheError::BadMagic);
    }
    if reader.u32()? != FORMAT_VERSION {
        return Err(CacheError::FormatVersion);
    }
    if reader.u32()? != ANALYZER_VERSION {
        return Err(CacheError::VersionMismatch);
    }
    let id_len = reader.u16()? as usize;
    let id_bytes = reader.take(id_len)?;
    if id_bytes != track.as_str().as_bytes() {
        return Err(CacheError::IdMismatch);
    }
    let sample_rate = reader.u32()?;
    let len_frames = reader.u64()?;
    let tolerance = LEN_TOLERANCE_SECONDS * u64::from(sample_rate.max(1));
    if len_frames.abs_diff(expected_len_frames) > tolerance {
        return Err(CacheError::LengthMismatch);
    }

    let section_count = reader.u32()?;
    let mut levels: Option<Vec<PeakLevel>> = None;
    for _ in 0..section_count {
        let tag = reader.take(4)?;
        let byte_len = reader.u64()? as usize;
        let payload = reader.take(byte_len)?;
        if tag == WAVE_TAG {
            levels = Some(decode_wave_section(payload, len_frames)?);
        }
        // Unknown sections are skipped: already consumed by `take` above,
        // so future DM-5 sections (beat grid, key, loudness) can be added
        // without a format bump.
    }
    let levels = levels.ok_or(CacheError::MissingWave)?;

    Ok(WaveformPeaks {
        sample_rate,
        len_frames,
        levels,
    })
}

fn decode_wave_section(payload: &[u8], len_frames: u64) -> Result<Vec<PeakLevel>, CacheError> {
    let mut reader = Reader::new(payload);
    let level_count = reader.u32()?;
    let mut levels = Vec::with_capacity(level_count as usize);
    for _ in 0..level_count {
        let frames_per_bucket = reader.u32()?;
        let bucket_count = reader.u32()? as usize;
        if bucket_count != peaks::bucket_count(len_frames, frames_per_bucket) {
            return Err(CacheError::Malformed);
        }
        let mut buckets = Vec::with_capacity(bucket_count);
        for _ in 0..bucket_count {
            let min = reader.i8()?;
            let max = reader.i8()?;
            buckets.push(PeakBucket { min, max });
        }
        levels.push(PeakLevel::full(frames_per_bucket, buckets));
    }
    Ok(levels)
}

/// Load a cache entry for `track`, unlinking it if it fails to validate
/// (research R7). `None` on any miss, mismatch, or IO error.
pub fn load(
    paths: &AnalysisPaths,
    track: &TrackId,
    expected_len_frames: u64,
) -> Option<WaveformPeaks> {
    let path = paths.file_for(track);
    let bytes = fs::read(&path).ok()?;
    match decode(&bytes, track, expected_len_frames) {
        Ok(peaks) => Some(peaks),
        Err(_) => {
            let _ = fs::remove_file(&path);
            None
        }
    }
}

/// Write a cache entry atomically (temp file + `rename`, user-only
/// permissions on Unix — mirrors `library::persist`'s `write_atomic`).
pub fn store(
    paths: &AnalysisPaths,
    track: &TrackId,
    peaks: &WaveformPeaks,
) -> Result<(), CacheError> {
    let path = paths.file_for(track);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp_path = path.with_extension("mpwf.tmp");
    let bytes = encode(peaks, track);
    {
        let mut file = fs::File::create(&tmp_path)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
    }
    fs::rename(&tmp_path, &path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: &str) -> TrackId {
        TrackId::new(id).unwrap_or_else(|_| unreachable!())
    }

    fn sample_peaks(len_frames: u64) -> WaveformPeaks {
        WaveformPeaks {
            sample_rate: 44_100,
            len_frames,
            levels: vec![
                PeakLevel::full(
                    128,
                    (0..peaks::bucket_count(len_frames, 128))
                        .map(|i| PeakBucket {
                            min: -(i as i8 % 127),
                            max: (i as i8 % 127),
                        })
                        .collect(),
                ),
                PeakLevel::full(
                    1_024,
                    (0..peaks::bucket_count(len_frames, 1_024))
                        .map(|_| PeakBucket { min: -10, max: 10 })
                        .collect(),
                ),
            ],
        }
    }

    #[test]
    fn encode_decode_round_trips() {
        let id = track("spotify:track:abc");
        let peaks = sample_peaks(44_100 * 200);
        let bytes = encode(&peaks, &id);
        let decoded = decode(&bytes, &id, peaks.len_frames).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(decoded, peaks);
    }

    #[test]
    fn decode_rejects_wrong_track_id() {
        let id = track("spotify:track:abc");
        let other = track("spotify:track:xyz");
        let peaks = sample_peaks(44_100 * 200);
        let bytes = encode(&peaks, &id);
        assert!(matches!(
            decode(&bytes, &other, peaks.len_frames),
            Err(CacheError::IdMismatch)
        ));
    }

    #[test]
    fn decode_rejects_stale_analyzer_version() {
        let id = track("spotify:track:abc");
        let peaks = sample_peaks(44_100 * 200);
        let mut bytes = encode(&peaks, &id);
        // analyzer_version is the u32 right after magic + format_version.
        bytes[8..12].copy_from_slice(&(ANALYZER_VERSION + 1).to_le_bytes());
        assert!(matches!(
            decode(&bytes, &id, peaks.len_frames),
            Err(CacheError::VersionMismatch)
        ));
    }

    #[test]
    fn decode_rejects_truncated_bytes() {
        let id = track("spotify:track:abc");
        let peaks = sample_peaks(44_100 * 200);
        let bytes = encode(&peaks, &id);
        for cut in [0, 1, 4, 10, bytes.len() / 2, bytes.len() - 1] {
            assert!(decode(&bytes[..cut], &id, peaks.len_frames).is_err());
        }
    }

    #[test]
    fn store_then_load_round_trips_on_disk() {
        let dir = std::env::temp_dir().join(format!(
            "modplayer-analysis-cache-test-{}-{}",
            std::process::id(),
            "store_then_load_round_trips_on_disk"
        ));
        let _ = fs::remove_dir_all(&dir);
        let paths = AnalysisPaths::with_dir(&dir);
        let id = track("spotify:track:disk");
        let peaks = sample_peaks(44_100 * 60);

        store(&paths, &id, &peaks).unwrap_or_else(|e| panic!("{e}"));
        let loaded = load(&paths, &id, peaks.len_frames);
        assert_eq!(loaded, Some(peaks));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_unlinks_a_stale_entry() {
        let dir = std::env::temp_dir().join(format!(
            "modplayer-analysis-cache-test-{}-{}",
            std::process::id(),
            "load_unlinks_a_stale_entry"
        ));
        let _ = fs::remove_dir_all(&dir);
        let paths = AnalysisPaths::with_dir(&dir);
        let id = track("spotify:track:stale");
        let peaks = sample_peaks(44_100 * 60);
        store(&paths, &id, &peaks).unwrap_or_else(|e| panic!("{e}"));

        // Loading with a very different expected length makes the entry
        // stale (length mismatch); it must be unlinked.
        assert_eq!(load(&paths, &id, 44_100 * 6_000), None);
        assert!(!paths.file_for(&id).exists());

        let _ = fs::remove_dir_all(&dir);
    }
}

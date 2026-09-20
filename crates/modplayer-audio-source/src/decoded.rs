// SPDX-License-Identifier: MIT OR Apache-2.0

//! `DecodedStore`: the shared, lock-free, growable ring of decoded PCM
//! samples for the current track, written by the decode-ahead thread and
//! read by the real-time receiver feed and the off-real-time Analysis
//! Service (005-now-playing-waveform, contracts/decoded-store.md §1-2,
//! data-model.md §2).
//!
//! Constitution V boundary: `read_frames` (the only raw-sample accessor)
//! is `#[doc(hidden)]` and callable only from
//! `modplayer-audio-source*`/`modplayer-engine` — mechanically enforced by
//! `crates/modplayer/tests/decoded_store_boundary.rs`, which scans every
//! other crate's source for the literal `read_frames(`. The analysis path
//! (`modplayer-core`, `modplayer-ui`) uses only `fold_peaks`/`coverage`/
//! `covers`, all lossy/non-invertible.

use std::fmt;
use std::sync::atomic::{AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

/// Frames per chunk (contracts/decoded-store.md §1). `65_536` frames ≈
/// 1.49 s at 44.1 kHz.
pub const CHUNK_FRAMES: u64 = 65_536;

/// Absolute cap on a store's decoded length: 256 MiB of stereo `f32`
/// (contracts/decoded-store.md §1, rule 4).
pub const MAX_STORE_FRAMES: u64 = 33_554_432;

/// A store's lifecycle (data-model.md §1.1). Terminal once `Complete` or
/// `Failed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreState {
    Filling,
    Complete,
    Failed,
}

impl StoreState {
    fn to_u8(self) -> u8 {
        match self {
            StoreState::Filling => 0,
            StoreState::Complete => 1,
            StoreState::Failed => 2,
        }
    }

    fn from_u8(value: u8) -> Self {
        match value {
            1 => StoreState::Complete,
            2 => StoreState::Failed,
            _ => StoreState::Filling,
        }
    }
}

/// One folded min/max peak, quantised to `i8` (contracts/decoded-store.md
/// §1, rule 6): `round(clamp(sample, -1, 1) * 127)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PeakBucket {
    pub min: i8,
    pub max: i8,
}

/// One `CHUNK_FRAMES`-sized slot. `samples` is allocated lazily (on first
/// write) so a store's initial allocation is the slot table only
/// (data-model.md §1.1's `new`). `filled` = frames valid from the chunk's
/// first frame (contracts/decoded-store.md §2, rule 1): a chunk is always
/// filled from its start.
struct Chunk {
    samples: OnceLock<Box<[AtomicU32]>>,
    filled: AtomicU32,
}

impl Chunk {
    fn new() -> Self {
        Self {
            samples: OnceLock::new(),
            filled: AtomicU32::new(0),
        }
    }
}

/// The shared, lock-free decoded-PCM store for the current track
/// (contracts/decoded-store.md §1). Constructed once per track by a
/// `SourceHost` implementor's writer thread, filled append-only, and read
/// by the real-time feed (`read_frames`, sample-exact) and the Analysis
/// Service (`fold_peaks`, lossy). The RT half never drops the last `Arc`
/// (research R1) — see contracts/connect-source-delta.md for the
/// retirement-ring discipline that guarantees this.
pub struct DecodedStore {
    sample_rate: u32,
    len_frames: AtomicU64,
    state: AtomicU8,
    chunks: Box<[Chunk]>,
}

impl DecodedStore {
    /// Allocate the slot table for a track of (best-known) `len_frames` at
    /// `sample_rate`. Only the table itself is allocated here — chunk
    /// sample arrays are allocated lazily on first write. `len_frames` is
    /// clamped to `MAX_STORE_FRAMES` when sizing the table (rule 4); a
    /// track whose real length exceeds the cap can never reach `Complete`
    /// (it stays `Filling`/`Partial` for the session).
    pub fn new(sample_rate: u32, len_frames: u64) -> Arc<Self> {
        let capped = len_frames.min(MAX_STORE_FRAMES);
        let n_chunks = capped.div_ceil(CHUNK_FRAMES).max(1);
        let chunks: Vec<Chunk> = (0..n_chunks).map(|_| Chunk::new()).collect();
        Arc::new(Self {
            sample_rate,
            len_frames: AtomicU64::new(len_frames),
            state: AtomicU8::new(StoreState::Filling.to_u8()),
            chunks: chunks.into_boxed_slice(),
        })
    }

    /// Sample rate the store was created at; constant for its lifetime.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Best-known track length: the `new()` estimate until `set_complete`
    /// refines it to the exact decoded length.
    pub fn len_frames(&self) -> u64 {
        self.len_frames.load(Ordering::SeqCst)
    }

    /// Current lifecycle state.
    pub fn state(&self) -> StoreState {
        StoreState::from_u8(self.state.load(Ordering::SeqCst))
    }

    /// Total slots this store's table was sized for (`<= MAX_STORE_FRAMES`,
    /// contracts/decoded-store.md §1, rule 4) — the effective per-store cap
    /// `set_complete` checks against.
    fn capacity_frames(&self) -> u64 {
        self.chunks.len() as u64 * CHUNK_FRAMES
    }

    /// Whether `frame` has been written and published (contracts/
    /// decoded-store.md §2, rule 2: `Acquire`-ordered, sees only fully
    /// published writes). `false` beyond `len_frames()` or the cap.
    pub fn covers(&self, frame: u64) -> bool {
        if frame >= self.len_frames() {
            return false;
        }
        let chunk_index = (frame / CHUNK_FRAMES) as usize;
        let Some(chunk) = self.chunks.get(chunk_index) else {
            return false;
        };
        let offset = frame % CHUNK_FRAMES;
        u64::from(chunk.filled.load(Ordering::Acquire)) > offset
    }

    /// Sum of every chunk's `filled` (lossy: does not itself express which
    /// specific frames are covered when there are multiple gaps — use
    /// `covers`/`coverage` for that).
    pub fn covered_frames(&self) -> u64 {
        self.chunks
            .iter()
            .map(|chunk| u64::from(chunk.filled.load(Ordering::Acquire)))
            .sum()
    }

    /// `filled` per chunk, in order (contracts/decoded-store.md §2's
    /// `coverage`).
    pub fn coverage(&self) -> Vec<u32> {
        self.chunks
            .iter()
            .map(|chunk| chunk.filled.load(Ordering::Acquire))
            .collect()
    }

    /// Raw stereo sample pair at `frame`, or `None` if not covered.
    /// Private: the *only* raw-sample paths out of this type are this
    /// function's two callers below (`read_frames`, real-time only, and
    /// `fold_peaks`, lossy fold only) — Constitution V is satisfied by
    /// construction, not by convention (contracts/decoded-store.md §4).
    fn sample_pair(&self, frame: u64) -> Option<(f32, f32)> {
        if frame >= self.len_frames() {
            return None;
        }
        let chunk_index = (frame / CHUNK_FRAMES) as usize;
        let chunk = self.chunks.get(chunk_index)?;
        let offset = (frame % CHUNK_FRAMES) as usize;
        let filled = chunk.filled.load(Ordering::Acquire) as usize;
        if offset >= filled {
            return None;
        }
        let samples = chunk.samples.get()?;
        let l = f32::from_bits(samples[offset * 2].load(Ordering::Relaxed));
        let r = f32::from_bits(samples[offset * 2 + 1].load(Ordering::Relaxed));
        Some((l, r))
    }

    /// Copy up to `out.len() / 2` *contiguous* covered frames starting at
    /// `from_frame`, stopping at the first uncovered frame. No allocation,
    /// no lock — the real-time contract (contracts/decoded-store.md §2,
    /// rule 3). Callers outside `modplayer-audio-source*`/
    /// `modplayer-engine` are mechanically forbidden (§4).
    #[doc(hidden)]
    pub fn read_frames(&self, from_frame: u64, out: &mut [f32]) -> usize {
        let want_frames = out.len() / 2;
        let mut copied = 0usize;
        while copied < want_frames {
            match self.sample_pair(from_frame + copied as u64) {
                Some((l, r)) => {
                    out[copied * 2] = l;
                    out[copied * 2 + 1] = r;
                    copied += 1;
                }
                None => break,
            }
        }
        copied
    }

    /// Write `interleaved` (stereo `f32`, contiguous frames) starting at
    /// `from_frame`. Writer-thread only (one writer per store).
    ///
    /// The write's *start* must land exactly on a chunk's first frame or
    /// immediately after its current `filled` watermark (append); a
    /// misaligned start writes nothing and returns 0 (contracts/
    /// decoded-store.md §2, rule 1). A start that validates may still
    /// straddle several chunks in one call (a decode-ahead read is rarely
    /// chunk-sized) — each successive chunk this call reaches must itself
    /// already be exactly at its own append point, which holds for any
    /// contiguous decode; if some later chunk turns out already ahead
    /// (concurrent/out-of-order writer misuse) this call simply stops
    /// there, returning the frames it *did* validly write rather than
    /// rolling those already-published, already-observable-by-readers
    /// frames back to satisfy a stricter all-or-nothing contract.
    pub fn write_frames(&self, from_frame: u64, interleaved: &[f32]) -> usize {
        if self.state() != StoreState::Filling {
            return 0;
        }
        let total_frames = interleaved.len() / 2;
        if total_frames == 0 {
            return 0;
        }

        let mut written = 0usize;
        let mut frame = from_frame;
        let mut src_idx = 0usize;
        while src_idx < total_frames && frame < MAX_STORE_FRAMES {
            let chunk_index = (frame / CHUNK_FRAMES) as usize;
            let Some(chunk) = self.chunks.get(chunk_index) else {
                break;
            };
            let chunk_start = chunk_index as u64 * CHUNK_FRAMES;
            let offset = (frame - chunk_start) as usize;
            let filled = chunk.filled.load(Ordering::Acquire) as usize;
            if offset != filled {
                // Not an append point for this chunk (rule 1): the very
                // first chunk touched failing this check is exactly
                // `store_rejects_non_aligned_writes` — nothing was written
                // yet, so `written` is still 0 here.
                break;
            }

            let samples = chunk.samples.get_or_init(|| {
                (0..CHUNK_FRAMES * 2)
                    .map(|_| AtomicU32::new(0))
                    .collect::<Vec<_>>()
                    .into_boxed_slice()
            });

            let space = CHUNK_FRAMES as usize - offset;
            let remaining_src = total_frames - src_idx;
            let n = space.min(remaining_src);
            for i in 0..n {
                let l = interleaved[(src_idx + i) * 2];
                let r = interleaved[(src_idx + i) * 2 + 1];
                // Relaxed: publication happens via the single `Release`
                // store of `filled` immediately below (rule 2).
                samples[(offset + i) * 2].store(l.to_bits(), Ordering::Relaxed);
                samples[(offset + i) * 2 + 1].store(r.to_bits(), Ordering::Relaxed);
            }
            chunk.filled.store((offset + n) as u32, Ordering::Release);

            written += n;
            src_idx += n;
            frame += n as u64;
        }
        written
    }

    /// Transition to `Complete`, refining `len_frames` to
    /// `exact_len_frames` (data-model.md §1.1). A no-op (state stays
    /// `Filling`) when `exact_len_frames` exceeds this store's capacity
    /// (the track is longer than the cap) or when any slot within
    /// `exact_len_frames` is still unfilled (contracts/decoded-store.md
    /// §2, rule 4) — such a store stays `Filling`/`Partial` for the
    /// session. Already-terminal states are unaffected (rule 5).
    pub fn set_complete(&self, exact_len_frames: u64) {
        if self.state() != StoreState::Filling {
            return;
        }
        if exact_len_frames > self.capacity_frames() {
            return;
        }
        if self.covered_frames() < exact_len_frames {
            return;
        }
        self.len_frames.store(exact_len_frames, Ordering::SeqCst);
        self.state
            .store(StoreState::Complete.to_u8(), Ordering::SeqCst);
    }

    /// Transition to `Failed` (terminal, rule 5); a no-op once already
    /// `Complete` or `Failed`.
    pub fn set_failed(&self) {
        if self.state() != StoreState::Filling {
            return;
        }
        self.state
            .store(StoreState::Failed.to_u8(), Ordering::SeqCst);
    }

    /// Fold whole, fully-covered buckets of `bucket_frames` frames each,
    /// starting at `from_frame`, into `out` (contracts/decoded-store.md
    /// §2, rule 6). Mono fold across both channels: `min = min(minL,
    /// minR)`, `max = max(maxL, maxR)`, quantised to `i8`. Stops at the
    /// first bucket with an uncovered frame (so callers can advance a
    /// watermark one pass at a time); the last bucket before `len_frames`
    /// may be short. Returns the number of buckets written.
    pub fn fold_peaks(&self, from_frame: u64, bucket_frames: u32, out: &mut [PeakBucket]) -> usize {
        let bucket_frames = u64::from(bucket_frames.max(1));
        let len = self.len_frames();
        let mut written = 0usize;
        let mut start = from_frame;
        while written < out.len() && start < len {
            let end = (start + bucket_frames).min(len);
            let mut min_v: i8 = 127;
            let mut max_v: i8 = -127;
            let mut frame = start;
            let mut fully_covered = true;
            while frame < end {
                match self.sample_pair(frame) {
                    Some((l, r)) => {
                        let ql = quantise(l);
                        let qr = quantise(r);
                        min_v = min_v.min(ql).min(qr);
                        max_v = max_v.max(ql).max(qr);
                    }
                    None => {
                        fully_covered = false;
                        break;
                    }
                }
                frame += 1;
            }
            if !fully_covered {
                break;
            }
            out[written] = PeakBucket {
                min: min_v,
                max: max_v,
            };
            written += 1;
            start = end;
        }
        written
    }
}

/// `round(clamp(sample, -1, 1) * 127)` (contracts/decoded-store.md §2,
/// rule 6).
fn quantise(sample: f32) -> i8 {
    let clamped = sample.clamp(-1.0, 1.0);
    let scaled = (clamped * 127.0).round();
    // `scaled` is always in `[-127.0, 127.0]` by construction (the clamp
    // above), so this cast never truncates a value outside `i8`'s range.
    scaled as i8
}

impl fmt::Debug for DecodedStore {
    /// Redacted: rate, length, state, covered-frame count only — never
    /// sample values (Constitution V; contracts/decoded-store.md §1).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DecodedStore")
            .field("sample_rate", &self.sample_rate)
            .field("len_frames", &self.len_frames())
            .field("state", &self.state())
            .field("covered_frames", &self.covered_frames())
            .finish()
    }
}

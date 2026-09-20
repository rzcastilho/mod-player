// SPDX-License-Identifier: MIT OR Apache-2.0

//! The decode-ahead thread: decodes the current track's audio via
//! `symphonia` (through `Subfile`/`AudioDecrypt`) into its `DecodedStore`,
//! ahead of real-time playback, at below-normal OS priority
//! (005-now-playing-waveform, research R3/R9, contracts/
//! connect-source-delta.md §1).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use librespot_audio::{AudioDecrypt, AudioFetchParams, AudioFile};
use librespot_core::{Error as LibrespotError, FileId, Session, SpotifyId};
use librespot_metadata::audio::{AudioFileFormat, AudioFiles, AudioItem};
use modplayer_audio_source::{CHUNK_FRAMES, DecodedStore};
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::{FormatOptions, SeekMode, SeekTo};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::rt::SAMPLE_RATE;
use crate::subfile::Subfile;

/// The `Player`'s own Bitrate160 format preference list, replicated
/// exactly (contracts/connect-source-delta.md §1) so the decode-ahead
/// resolves the *same* `(format, file_id)` librespot's `Player` already
/// picked.
const FORMAT_PREFERENCE: [AudioFileFormat; 7] = [
    AudioFileFormat::OGG_VORBIS_160,
    AudioFileFormat::MP3_160,
    AudioFileFormat::OGG_VORBIS_96,
    AudioFileFormat::MP3_96,
    AudioFileFormat::MP3_256,
    AudioFileFormat::OGG_VORBIS_320,
    AudioFileFormat::MP3_320,
];

/// Spotify's own Ogg container header length before the audio payload
/// starts (contracts/connect-source-delta.md §1).
const SPOTIFY_OGG_HEADER_END: u64 = 0xa7;

/// `MODPLAYER_DECODE_FORCE_FAIL` (quickstart M15): a debug-only toggle
/// mirroring 004's `MODPLAYER_ARTWORK_FORCE_FAIL` pattern — read once at
/// worker spawn, receiver-only, never affects the `Player`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ForceFail {
    None,
    /// `=0s`: fail immediately.
    Immediate,
    /// Any other value: partial-then-failed after 5 s of decoded audio.
    After5s,
}

impl ForceFail {
    pub(crate) fn from_env() -> Self {
        match std::env::var("MODPLAYER_DECODE_FORCE_FAIL").ok().as_deref() {
            None => ForceFail::None,
            Some("0s") => ForceFail::Immediate,
            Some(_) => ForceFail::After5s,
        }
    }
}

/// A live decode-ahead thread's control handle (worker-owned).
pub(crate) struct DecodeAheadHandle {
    stop: Arc<AtomicBool>,
    /// `u64::MAX` = no pending hint (contracts/connect-source-delta.md
    /// §2's `seek_hint`).
    seek_hint: Arc<AtomicU64>,
    thread: Option<JoinHandle<()>>,
}

impl DecodeAheadHandle {
    /// `SourceCommand::Seek(ms)` → the equivalent frame, checked between
    /// packets (contracts/connect-source-delta.md §1).
    pub(crate) fn seek_hint(&self, frame: u64) {
        self.seek_hint.store(frame, Ordering::Release);
    }

    /// Signal the thread to stop and join it (best-effort; never blocks
    /// indefinitely in practice since the loop checks `stop` between every
    /// packet).
    pub(crate) fn stop_and_join(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for DecodeAheadHandle {
    fn drop(&mut self) {
        self.stop_and_join();
    }
}

// Exercised only from `tests/worker.rs`, which `#[path]`-includes this
// file as its own copy of the module (so it can drive `seek_hint`/
// `forward_prefetch_hint` without a live `Spirc` session) — invisible to
// dead-code analysis of *this* crate's own `#[cfg(test)]` unit tests, so
// both methods are allowed dead here.
#[cfg(test)]
#[allow(dead_code)]
impl DecodeAheadHandle {
    /// A handle with no live thread behind it — 006's forwarding tests
    /// exercise `seek_hint`/`forward_prefetch_hint` without spawning a
    /// real decode (which needs a live `Session`/`AudioItem`).
    pub(crate) fn for_test() -> Self {
        Self {
            stop: Arc::new(AtomicBool::new(false)),
            seek_hint: Arc::new(AtomicU64::new(u64::MAX)),
            thread: None,
        }
    }

    /// The last frame stored by `seek_hint` (`u64::MAX` = none yet).
    pub(crate) fn pending_seek_hint(&self) -> u64 {
        self.seek_hint.load(Ordering::Acquire)
    }
}

/// The worker's single-slot decode-ahead handle, shared with its command
/// loop (006, contracts/engine-loop.md §1's `PrefetchHint` forwarding;
/// mirrors `crates/modplayer-audio-source-connect/src/worker.rs`'s own
/// alias of the same type).
pub(crate) type SharedDecodeAhead = Arc<Mutex<Option<DecodeAheadHandle>>>;

/// Forward `frame` to the current decode-ahead's `seek_hint`, if one is
/// running — a no-op otherwise (006, contracts/engine-loop.md §1). Never
/// touches `Spirc`/`Player`; a free function (rather than inline in
/// `worker.rs`'s match arm) so it is testable without a live `Spirc`
/// session (`tests/worker.rs`).
pub(crate) fn forward_prefetch_hint(decode_ahead: &SharedDecodeAhead, frame: u64) {
    if let Some(handle) = decode_ahead
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_ref()
    {
        handle.seek_hint(frame);
    }
}

/// Spawn the decode-ahead thread for `audio_item`, filling `store`
/// (contracts/connect-source-delta.md §1). Never touches the ring, the
/// marker ring, `Spirc`, or the `Player`.
pub(crate) fn spawn(
    session: Session,
    audio_item: AudioItem,
    store: Arc<DecodedStore>,
    runtime: tokio::runtime::Handle,
    force_fail: ForceFail,
) -> DecodeAheadHandle {
    let stop = Arc::new(AtomicBool::new(false));
    let seek_hint = Arc::new(AtomicU64::new(u64::MAX));
    let stop_thread = Arc::clone(&stop);
    let seek_hint_thread = Arc::clone(&seek_hint);

    let builder = thread::Builder::new().name("decode-ahead".to_string());
    let thread = builder
        .spawn(move || {
            if let Err(error) =
                thread_priority::set_current_thread_priority(thread_priority::ThreadPriority::Min)
            {
                // Non-fatal (R9): decode still proceeds, just at normal
                // priority.
                let _ = error;
            }
            run(
                &session,
                &audio_item,
                &store,
                &stop_thread,
                &seek_hint_thread,
                &runtime,
                force_fail,
            );
        })
        .ok();

    DecodeAheadHandle {
        stop,
        seek_hint,
        thread,
    }
}

/// First of `FORMAT_PREFERENCE` present in `audio_item.files`.
fn pick_format(audio_item: &AudioItem) -> Option<(AudioFileFormat, FileId)> {
    FORMAT_PREFERENCE
        .iter()
        .find_map(|&format| audio_item.files.get(&format).map(|&id| (format, id)))
}

/// Nominal bytes/second for `AudioFile::open`'s read-ahead pacing
/// (mirrors librespot-playback's own `PlayerTrackLoader::stream_data_rate`
/// for the formats this decode-ahead ever picks).
fn stream_data_rate(format: AudioFileFormat) -> usize {
    let kbps: f32 = match format {
        AudioFileFormat::OGG_VORBIS_96 | AudioFileFormat::MP3_96 => 12.0,
        AudioFileFormat::OGG_VORBIS_160 | AudioFileFormat::MP3_160 => 20.0,
        AudioFileFormat::MP3_256 => 32.0,
        AudioFileFormat::OGG_VORBIS_320 | AudioFileFormat::MP3_320 => 40.0,
        _ => 20.0,
    };
    (kbps * 1024.0).ceil() as usize
}

/// The lowest chunk-aligned frame this store hasn't fully covered yet, or
/// `None` once every chunk within `len_frames` is full (contracts/
/// connect-source-delta.md §1 "jump to lowest unfilled chunk").
fn lowest_unfilled_chunk_start(store: &DecodedStore) -> Option<u64> {
    let len = store.len_frames();
    for (index, filled) in store.coverage().iter().enumerate() {
        let chunk_start = index as u64 * CHUNK_FRAMES;
        if chunk_start >= len {
            break;
        }
        let expected = CHUNK_FRAMES.min(len - chunk_start);
        if u64::from(*filled) < expected {
            return Some(chunk_start);
        }
    }
    None
}

/// Write `samples` (interleaved stereo, starting at the container's
/// `packet_ts`) into `store`, discarding any leading frames before
/// `*cursor` (contracts/decoded-store.md rule 1 / research R3 step 2: a
/// chunk is only ever written from its own start, so frames a seek's
/// accurate-mode landed *before* the target must be trimmed). Advances
/// `*cursor` by however many frames were actually written.
fn write_trimmed(store: &DecodedStore, cursor: &mut u64, packet_ts: u64, samples: &[f32]) {
    let frame_count = samples.len() / 2;
    if frame_count == 0 {
        return;
    }
    let mut start_frame = packet_ts;
    let mut skip_frames = 0usize;
    if start_frame < *cursor {
        skip_frames = (*cursor - start_frame) as usize;
        start_frame = *cursor;
    }
    if skip_frames >= frame_count {
        return;
    }
    let written = store.write_frames(start_frame, &samples[skip_frames * 2..]);
    *cursor = start_frame + written as u64;
}

/// Resolve `audio_item.track_id` to the `SpotifyId` `audio_key().request`
/// needs. `None` for anything the key service can't be asked about
/// (matches librespot-playback's own "continue without decryption" path).
fn request_key(
    runtime: &tokio::runtime::Handle,
    session: &Session,
    audio_item: &AudioItem,
    file_id: FileId,
) -> Option<librespot_core::audio_key::AudioKey> {
    let track_id: SpotifyId = (&audio_item.track_id).try_into().ok()?;
    let result: Result<_, LibrespotError> =
        runtime.block_on(session.audio_key().request(track_id, file_id));
    result.ok()
}

#[allow(clippy::too_many_lines)]
fn run(
    session: &Session,
    audio_item: &AudioItem,
    store: &Arc<DecodedStore>,
    stop: &Arc<AtomicBool>,
    seek_hint: &Arc<AtomicU64>,
    runtime: &tokio::runtime::Handle,
    force_fail: ForceFail,
) {
    let Some((format, file_id)) = pick_format(audio_item) else {
        store.set_failed();
        return;
    };
    let bytes_per_second = stream_data_rate(format);

    let encrypted = match runtime.block_on(AudioFile::open(session, file_id, bytes_per_second)) {
        Ok(file) => file,
        Err(_) => {
            store.set_failed();
            return;
        }
    };
    let length = match encrypted.get_stream_loader_controller() {
        Ok(controller) => controller.len() as u64,
        Err(_) => {
            store.set_failed();
            return;
        }
    };
    let key = request_key(runtime, session, audio_item, file_id);
    let decrypted = AudioDecrypt::new(key, encrypted);

    let is_ogg = AudioFiles::is_ogg_vorbis(format);
    let offset = if is_ogg { SPOTIFY_OGG_HEADER_END } else { 0 };
    let subfile = match Subfile::new(decrypted, offset, length) {
        Ok(subfile) => subfile,
        Err(_) => {
            store.set_failed();
            return;
        }
    };

    let _ = AudioFetchParams::get(); // ensure the process-global params exist (configured once in lib.rs)

    let mss = MediaSourceStream::new(Box::new(subfile), MediaSourceStreamOptions::default());
    let mut hint = Hint::new();
    if let Some(mime) = AudioFiles::mime_type(format) {
        hint.mime_type(mime);
    }
    let format_opts = FormatOptions {
        enable_gapless: true,
        ..FormatOptions::default()
    };
    let metadata_opts = MetadataOptions::default();
    let probed =
        match symphonia::default::get_probe().format(&hint, mss, &format_opts, &metadata_opts) {
            Ok(probed) => probed,
            Err(_) => {
                store.set_failed();
                return;
            }
        };
    let mut format_reader = probed.format;
    let Some(track) = format_reader.default_track().cloned() else {
        store.set_failed();
        return;
    };
    let track_id_num = track.id;
    if track.codec_params.channels.map(|c| c.count()) != Some(2)
        || track.codec_params.sample_rate != Some(SAMPLE_RATE)
    {
        // This slice's store is fixed at 44.1 kHz stereo (matches
        // `ConnectRtSource::SAMPLE_RATE`, always true for a Spotify-
        // delivered file); anything else can't be folded into it.
        store.set_failed();
        return;
    }
    let decoder_opts = DecoderOptions::default();
    let mut decoder =
        match symphonia::default::get_codecs().make(&track.codec_params, &decoder_opts) {
            Ok(decoder) => decoder,
            Err(_) => {
                store.set_failed();
                return;
            }
        };

    let mut cursor: u64 = 0;
    let mut sample_buf: Option<symphonia::core::audio::SampleBuffer<f32>> = None;

    loop {
        if stop.load(Ordering::Acquire) {
            return;
        }
        match force_fail {
            ForceFail::Immediate => {
                store.set_failed();
                return;
            }
            ForceFail::After5s if store.covered_frames() >= u64::from(SAMPLE_RATE) * 5 => {
                store.set_failed();
                return;
            }
            _ => {}
        }

        let hinted = seek_hint.swap(u64::MAX, Ordering::AcqRel);
        if hinted != u64::MAX && !store.covers(hinted) {
            let chunk_start = (hinted / CHUNK_FRAMES) * CHUNK_FRAMES;
            if let Ok(seeked) = format_reader.seek(
                SeekMode::Accurate,
                SeekTo::TimeStamp {
                    ts: chunk_start,
                    track_id: track_id_num,
                },
            ) {
                cursor = seeked.actual_ts.min(chunk_start);
                decoder.reset();
            }
            // A seek error is not fatal here (research R3): continue
            // sequentially from wherever the reader still is.
        }

        if store.covers(cursor) {
            match lowest_unfilled_chunk_start(store) {
                Some(target) => {
                    match format_reader.seek(
                        SeekMode::Accurate,
                        SeekTo::TimeStamp {
                            ts: target,
                            track_id: track_id_num,
                        },
                    ) {
                        Ok(seeked) => {
                            cursor = seeked.actual_ts.min(target);
                            decoder.reset();
                        }
                        Err(_) => cursor = target,
                    }
                }
                None => {
                    store.set_complete(store.covered_frames());
                    return;
                }
            }
        }

        let packet = match format_reader.next_packet() {
            Ok(packet) => packet,
            Err(SymphoniaError::IoError(io_error))
                if io_error.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                match lowest_unfilled_chunk_start(store) {
                    Some(target) => {
                        match format_reader.seek(
                            SeekMode::Accurate,
                            SeekTo::TimeStamp {
                                ts: target,
                                track_id: track_id_num,
                            },
                        ) {
                            Ok(seeked) => {
                                cursor = seeked.actual_ts.min(target);
                                decoder.reset();
                            }
                            Err(_) => {
                                store.set_complete(store.covered_frames());
                                return;
                            }
                        }
                    }
                    None => {
                        store.set_complete(store.covered_frames());
                        return;
                    }
                }
                continue;
            }
            Err(_) => {
                store.set_failed();
                return;
            }
        };

        match decoder.decode(&packet) {
            Ok(decoded) => {
                let buf = sample_buf.get_or_insert_with(|| {
                    symphonia::core::audio::SampleBuffer::new(
                        decoded.capacity() as u64,
                        *decoded.spec(),
                    )
                });
                buf.copy_interleaved_ref(decoded);
                write_trimmed(store, &mut cursor, packet.ts(), buf.samples());
            }
            // A single malformed packet is skipped, symphonia's own
            // convention (`SymphoniaDecoder::next_packet`) — try the next
            // one rather than failing the whole track.
            Err(SymphoniaError::DecodeError(_)) => {}
            Err(_) => {
                store.set_failed();
                return;
            }
        }
    }
}

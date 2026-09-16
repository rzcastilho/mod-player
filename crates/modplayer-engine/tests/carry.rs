// SPDX-License-Identifier: MIT OR Apache-2.0

//! `leftover_carry_is_bit_exact_with_001` (engine-delta.md §2, §5): the
//! leftover-carry replacement for the old rewind-by-`seek` exists so that
//! resampled output is independent of how a fixed total number of output
//! frames happens to be chunked across `render` calls — exactly the
//! invariant 001's rewind-by-`seek` guaranteed (each render always saw a
//! source re-fetched from the correct position, however small the
//! buffers). This golden-compares, for every `(source_rate, device_rate)`
//! pair and buffer size in the contract, a single one-shot render of N
//! frames against the same N frames rendered in buffer-size-sized chunks:
//! the two must be bit-identical.

use std::sync::Arc;

use modplayer_audio_source_synthetic::SyntheticSource;
use modplayer_engine::{
    CeilingDb, Command, Processor, ProcessorConfig, RtShared, Transport, VolumePercent,
};
use rtrb::RingBuffer;

/// Render `total_out_frames` device-rate frames from a fresh processor at
/// `(source_rate, device_rate)`, in chunks of `chunk_frames` (the last
/// chunk may be shorter), and return the interleaved-stereo output.
fn render_chunked(
    source_rate: u32,
    device_rate: u32,
    chunk_frames: usize,
    total_out_frames: usize,
) -> Vec<f32> {
    let (mut command_tx, command_rx) = RingBuffer::<Command>::new(256);
    let (event_tx, _event_rx) = RingBuffer::<modplayer_engine::Event>::new(256);
    let shared = Arc::new(RtShared::new());
    let config = ProcessorConfig {
        source_rate,
        device_rate,
        device_channels: 2,
        // Large enough that `required_source_frames` never needs to clamp
        // for any (source_rate, device_rate, chunk) combo exercised below.
        max_frames: 4096,
        transport: Transport::Playing,
        position_frames: 0,
        master_volume: VolumePercent::new(100),
        ceiling: CeilingDb::default(),
        shared,
    };
    let mut processor = Processor::new(
        config,
        SyntheticSource::new(source_rate),
        command_rx,
        event_tx,
    );
    let _ = command_tx.push(Command::Play);

    let mut out = Vec::with_capacity(total_out_frames * 2);
    let mut remaining = total_out_frames;
    while remaining > 0 {
        let this_chunk = remaining.min(chunk_frames);
        let mut buf = vec![0.0f32; this_chunk * 2];
        processor.render(&mut buf);
        out.extend_from_slice(&buf);
        remaining -= this_chunk;
    }
    out
}

#[test]
fn leftover_carry_is_bit_exact_across_chunking() {
    const TOTAL_OUT_FRAMES: usize = 1024 * 5;
    // An odd reference chunk size (not among the sizes under test, and
    // small enough never to exceed a single render call's documented
    // `MAX_FRAMES` bound) that a fixed-point resampler would only produce
    // bit-identical output for, across every other chunking, if the
    // leftover carry truly makes output chunking-independent.
    const REFERENCE_CHUNK: usize = 777;

    for &(source_rate, device_rate) in &[
        (44_100u32, 48_000u32),
        (44_100u32, 44_100u32),
        (48_000u32, 44_100u32),
    ] {
        let reference = render_chunked(source_rate, device_rate, REFERENCE_CHUNK, TOTAL_OUT_FRAMES);

        for &chunk in &[128usize, 256, 1024] {
            let chunked = render_chunked(source_rate, device_rate, chunk, TOTAL_OUT_FRAMES);
            assert_eq!(
                chunked, reference,
                "source_rate={source_rate} device_rate={device_rate} chunk={chunk}: \
                 output must be independent of how render calls are chunked"
            );
        }
    }
}

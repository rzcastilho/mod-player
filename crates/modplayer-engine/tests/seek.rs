// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Command::Seek` (engine-delta.md §1): pushed mid-run, the render that
//! drains it starts producing audio from the target frame — the
//! `SyntheticSource`'s deterministic, closed-form content makes this
//! directly observable.

use std::sync::Arc;

use modplayer_audio_source::AudioSource;
use modplayer_audio_source_synthetic::SyntheticSource;
use modplayer_engine::{
    CeilingDb, Command, Processor, ProcessorConfig, RtShared, Transport, VolumePercent,
};
use rtrb::RingBuffer;

#[test]
fn seek_command_applies_at_boundary() {
    let (mut command_tx, command_rx) = RingBuffer::<Command>::new(256);
    let (event_tx, _event_rx) = RingBuffer::<modplayer_engine::Event>::new(256);
    let shared = Arc::new(RtShared::new());
    let config = ProcessorConfig {
        source_rate: 44_100,
        device_rate: 44_100,
        device_channels: 2,
        max_frames: 256,
        transport: Transport::Playing,
        position_frames: 0,
        master_volume: VolumePercent::new(100),
        ceiling: CeilingDb::default(),
        shared: Arc::clone(&shared),
    };
    let mut processor = Processor::new(config, SyntheticSource::new(44_100), command_rx, event_tx);

    // Render a buffer before the seek: position should be exactly 256.
    let mut out = vec![0.0f32; 256 * 2];
    processor.render(&mut out);
    assert_eq!(shared.position_frames(), 256);

    // Push Seek(1000) between renders.
    let _ = command_tx.push(Command::Seek(1_000));

    // A separately constructed source at the target position produces the
    // same signal the seeked processor's next render must start from
    // (SyntheticSource's determinism guarantee).
    let mut reference = SyntheticSource::new(44_100);
    reference.seek(1_000);
    let mut expected = vec![0.0f32; 256 * 2];
    reference.fill(&mut expected);

    let mut after_seek = vec![0.0f32; 256 * 2];
    processor.render(&mut after_seek);
    assert_eq!(
        after_seek, expected,
        "render must start exactly at frame 1000"
    );
    assert_eq!(shared.position_frames(), 1_000 + 256);
}

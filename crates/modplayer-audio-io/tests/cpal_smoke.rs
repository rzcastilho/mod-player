// SPDX-License-Identifier: MIT OR Apache-2.0

//! Manual hardware smoke test for `CpalBackend` (contracts/output-backend.md):
//! enumerate real devices, open the default device at each buffer preset,
//! render one second of silence, and print the negotiated frame count.
//! Ignored by default (needs real audio hardware, unavailable in CI); run
//! explicitly with:
//!
//! ```text
//! cargo test -p modplayer-audio-io --test cpal_smoke -- --ignored --nocapture
//! ```

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use modplayer_audio_io::{CpalBackend, OutputBackend};
use modplayer_audio_source::AudioSource;
use modplayer_audio_source_synthetic::SyntheticSource;
use modplayer_engine::{
    BufferPreset, CeilingDb, Processor, ProcessorConfig, RtShared, Transport, VolumePercent,
};
use rtrb::RingBuffer;

#[test]
#[ignore = "manual: needs an audio device"]
fn enumerates_opens_and_renders_silence_on_the_default_device() {
    let mut backend = CpalBackend::new();

    let devices = match backend.devices() {
        Ok(devices) => devices,
        Err(e) => panic!("enumerate devices: {e}"),
    };
    println!("found {} output device(s)", devices.len());
    for device in &devices {
        println!("  {} (default: {})", device.name, device.is_default);
    }

    let default = match backend.default_device() {
        Ok(Some(device)) => device,
        Ok(None) => panic!("no default output device on this machine"),
        Err(e) => panic!("query default device: {e}"),
    };
    println!("default device: {}", default.name);

    for preset in [
        BufferPreset::Performance,
        BufferPreset::Balanced,
        BufferPreset::Safe,
    ] {
        let (command_tx, command_rx) = RingBuffer::new(256);
        let (event_tx, _event_rx) = RingBuffer::new(256);
        let shared = Arc::new(RtShared::new());
        let source = SyntheticSource::default();
        let config = ProcessorConfig {
            source_rate: source.sample_rate(),
            device_rate: default.default_rate.hz(),
            device_channels: default.channels,
            max_frames: preset.requested_frames().frames() as usize,
            transport: Transport::Stopped, // silent: no command producer needed
            position_frames: 0,
            master_volume: VolumePercent::new(0),
            ceiling: CeilingDb::default(),
            shared: Arc::clone(&shared),
        };
        let processor = Processor::new(config, source, command_rx, event_tx);
        drop(command_tx); // nothing to send this test

        let stream = match backend.open(&default.id, preset, processor) {
            Ok(stream) => stream,
            Err(e) => panic!("open at {preset:?} failed: {e}"),
        };
        println!(
            "{preset:?}: negotiated {} frames @ {} Hz",
            stream.negotiated.frames.frames(),
            stream.negotiated.device_rate.hz()
        );

        thread::sleep(Duration::from_secs(1));
        println!(
            "  RtShared::negotiated_frames() = {}",
            shared.negotiated_frames()
        );
        drop(stream);
    }
}

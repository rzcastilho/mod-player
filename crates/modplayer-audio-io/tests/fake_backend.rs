// SPDX-License-Identifier: MIT OR Apache-2.0

//! `FakeBackend` unit tests, one per contract method
//! (contracts/output-backend.md).

use std::sync::Arc;

use modplayer_audio_io::{AudioIoError, BackendEvent, FakeBackend, FakeDevice, OutputBackend};
use modplayer_audio_source_synthetic::SyntheticSource;
use modplayer_engine::{
    BufferPreset, CeilingDb, DeviceId, FrameCount, Processor, ProcessorConfig, RtShared,
    SampleRate, Transport, VolumePercent,
};
use rtrb::RingBuffer;

fn device(id: &str, is_default: bool) -> FakeDevice {
    FakeDevice {
        id: DeviceId::new(id).unwrap_or_else(|| unreachable!("test id is non-empty")),
        name: id.to_string(),
        rate: SampleRate::new(44_100),
        channels: 2,
        buffer_range: Some((FrameCount::new(32), FrameCount::new(2048))),
        is_default,
    }
}

fn processor_for(rate: u32) -> Processor<SyntheticSource> {
    let (_command_tx, command_rx) = RingBuffer::new(256);
    let (event_tx, _event_rx) = RingBuffer::new(256);
    let config = ProcessorConfig {
        source_rate: rate,
        device_rate: rate,
        device_channels: 2,
        max_frames: 256,
        transport: Transport::Playing,
        position_frames: 0,
        master_volume: VolumePercent::new(80),
        ceiling: CeilingDb::default(),
        shared: Arc::new(RtShared::new()),
    };
    Processor::new(config, SyntheticSource::new(rate), command_rx, event_tx)
}

#[test]
fn devices_lists_every_scripted_device() {
    let backend = FakeBackend::new(vec![device("a", true), device("b", false)]);
    let listed = backend.devices().unwrap_or_default();
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].id.as_str(), "a");
    assert_eq!(listed[1].id.as_str(), "b");
}

#[test]
fn default_device_is_the_one_flagged_default() {
    let backend = FakeBackend::new(vec![device("a", false), device("b", true)]);
    let default = backend.default_device().unwrap_or_default();
    assert_eq!(
        default.map(|d| d.id.as_str().to_string()),
        Some("b".to_string())
    );
}

#[test]
fn open_clamps_negotiated_frames_into_buffer_range() {
    let mut backend = FakeBackend::new(vec![device("a", true)]);
    let id = DeviceId::new("a").unwrap_or_else(|| unreachable!());
    // Safe preset requests 1024 frames; the scripted range tops out at 2048, so it fits unclamped.
    let stream = backend.open(&id, BufferPreset::Safe, processor_for(44_100));
    let stream = stream.unwrap_or_else(|e| panic!("open failed: {e}"));
    assert_eq!(stream.negotiated.frames.frames(), 1024);
}

#[test]
fn open_on_unknown_device_errors() {
    let mut backend = FakeBackend::new(vec![device("a", true)]);
    let missing = DeviceId::new("missing").unwrap_or_else(|| unreachable!());
    let result = backend.open(&missing, BufferPreset::Balanced, processor_for(44_100));
    assert!(matches!(result, Err(AudioIoError::NoSuchDevice(_))));
}

#[test]
fn render_buffers_drives_the_processor_and_concatenates_output() {
    let mut backend = FakeBackend::new(vec![device("a", true)]);
    let id = DeviceId::new("a").unwrap_or_else(|| unreachable!());
    let stream = backend
        .open(&id, BufferPreset::Performance, processor_for(44_100))
        .unwrap_or_else(|e| panic!("open failed: {e}"));
    let frames = stream.negotiated.frames.frames() as usize;

    let output = backend.render_buffers(3);
    assert_eq!(output.len(), frames * 2 * 3);
}

#[test]
fn render_buffers_is_empty_with_no_open_stream() {
    let mut backend = FakeBackend::new(vec![device("a", true)]);
    assert!(backend.render_buffers(5).is_empty());
}

#[test]
fn remove_active_device_emits_device_lost_then_list_changed() {
    let mut backend = FakeBackend::new(vec![device("a", true)]);
    let id = DeviceId::new("a").unwrap_or_else(|| unreachable!());
    let _ = backend.open(&id, BufferPreset::Balanced, processor_for(44_100));

    backend.remove_device(&id);

    let first = backend.events().try_recv().ok();
    let second = backend.events().try_recv().ok();
    assert_eq!(first, Some(BackendEvent::DeviceLost { id: id.clone() }));
    assert_eq!(second, Some(BackendEvent::DeviceListChanged));
    assert!(backend.devices().unwrap_or_default().is_empty());
    assert!(
        backend.render_buffers(1).is_empty(),
        "stream must be gone after removal"
    );
}

#[test]
fn remove_inactive_device_emits_nothing() {
    let mut backend = FakeBackend::new(vec![device("a", true), device("b", false)]);
    let b = DeviceId::new("b").unwrap_or_else(|| unreachable!());
    backend.remove_device(&b);
    assert!(backend.events().try_recv().is_err());
    assert_eq!(backend.devices().unwrap_or_default().len(), 1);
}

#[test]
fn add_device_emits_list_changed() {
    let mut backend = FakeBackend::new(vec![device("a", true)]);
    backend.add_device(device("c", false));
    assert_eq!(
        backend.events().try_recv().ok(),
        Some(BackendEvent::DeviceListChanged)
    );
    assert_eq!(backend.devices().unwrap_or_default().len(), 2);
}

#[test]
fn set_rate_emits_sample_rate_changed() {
    let mut backend = FakeBackend::new(vec![device("a", true)]);
    let id = DeviceId::new("a").unwrap_or_else(|| unreachable!());
    let new_rate = SampleRate::new(48_000);
    backend.set_rate(&id, new_rate);
    assert_eq!(
        backend.events().try_recv().ok(),
        Some(BackendEvent::SampleRateChanged {
            id: id.clone(),
            new_rate
        })
    );
    let listed = backend.devices().unwrap_or_default();
    assert_eq!(listed[0].default_rate, new_rate);
}

#[test]
fn set_default_moves_the_default_flag() {
    let mut backend = FakeBackend::new(vec![device("a", true), device("b", false)]);
    let b = DeviceId::new("b").unwrap_or_else(|| unreachable!());
    backend.set_default(&b);
    let listed = backend.devices().unwrap_or_default();
    assert!(!listed[0].is_default);
    assert!(listed[1].is_default);
}

#[test]
fn take_processor_detaches_without_events() {
    let mut backend = FakeBackend::new(vec![device("a", true)]);
    let id = DeviceId::new("a").unwrap_or_else(|| unreachable!());
    let _ = backend.open(&id, BufferPreset::Balanced, processor_for(44_100));

    let taken = backend.take_processor();
    assert!(taken);
    assert!(
        backend.events().try_recv().is_err(),
        "take_processor must not emit events"
    );
    assert!(!backend.take_processor(), "second take must find nothing");
}

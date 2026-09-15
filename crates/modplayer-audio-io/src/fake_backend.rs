// SPDX-License-Identifier: MIT OR Apache-2.0

//! `FakeBackend`: a scripted, headless `OutputBackend` implementation for
//! tests (research R5; contracts/output-backend.md). Drives its processor
//! synchronously — no thread, no platform dependency — so engine and
//! controller tests can run in CI without hardware (FR-024).

use std::sync::mpsc::{Receiver, Sender, channel};

use modplayer_audio_source_synthetic::SyntheticSource;
use modplayer_engine::{
    BufferPreset, DeviceId, FrameCount, NegotiatedBuffer, Processor, SampleRate,
};

use crate::backend::{BackendEvent, OpenStream, OutputBackend, OutputDeviceInfo, StreamHandle};
use crate::error::AudioIoError;

/// One scripted device in a `FakeBackend`'s device list.
#[derive(Debug, Clone, PartialEq)]
pub struct FakeDevice {
    pub id: DeviceId,
    pub name: String,
    pub rate: SampleRate,
    pub channels: u16,
    pub buffer_range: Option<(FrameCount, FrameCount)>,
    pub is_default: bool,
}

impl FakeDevice {
    fn to_info(&self) -> OutputDeviceInfo {
        OutputDeviceInfo {
            id: self.id.clone(),
            name: self.name.clone(),
            is_default: self.is_default,
            default_rate: self.rate,
            channels: self.channels,
            buffer_range: self.buffer_range,
        }
    }
}

/// Marker handle: `FakeBackend` owns and drives its processor directly
/// (via `render_buffers`), so dropping this handle has nothing further to
/// do — unlike `CpalBackend`, there is no real background stream to stop.
struct FakeStreamHandle;

impl StreamHandle for FakeStreamHandle {}

struct ActiveStream {
    device_id: DeviceId,
    device_channels: u16,
    negotiated: NegotiatedBuffer,
    processor: Processor<SyntheticSource>,
}

/// A scripted, in-process `OutputBackend` for tests.
pub struct FakeBackend {
    devices: Vec<FakeDevice>,
    active: Option<ActiveStream>,
    events_tx: Sender<BackendEvent>,
    events_rx: Receiver<BackendEvent>,
}

impl FakeBackend {
    /// Construct a backend with a scripted device list. Whichever device
    /// (if any) has `is_default: true` is reported as the default.
    pub fn new(devices: Vec<FakeDevice>) -> Self {
        let (events_tx, events_rx) = channel();
        Self {
            devices,
            active: None,
            events_tx,
            events_rx,
        }
    }

    /// Drive the active stream's processor for `n` buffers of
    /// `negotiated.frames` each, synchronously. Returns the concatenated,
    /// interleaved output (device channel count). Empty if no stream is open.
    pub fn render_buffers(&mut self, n: usize) -> Vec<f32> {
        let Some(active) = self.active.as_mut() else {
            return Vec::new();
        };
        let frames = active.negotiated.frames.frames() as usize;
        let channels = usize::from(active.device_channels);
        let mut output = Vec::with_capacity(frames * channels * n);
        let mut buffer = vec![0.0f32; frames * channels];
        for _ in 0..n {
            active.processor.render(&mut buffer);
            output.extend_from_slice(&buffer);
        }
        output
    }

    /// Remove a device from the list. If it was the active stream's
    /// device, the stream is dropped and `DeviceLost` then
    /// `DeviceListChanged` are emitted (contracts/output-backend.md).
    pub fn remove_device(&mut self, id: &DeviceId) {
        self.devices.retain(|d| &d.id != id);
        if self.active.as_ref().is_some_and(|a| &a.device_id == id) {
            self.active = None;
            let _ = self
                .events_tx
                .send(BackendEvent::DeviceLost { id: id.clone() });
            let _ = self.events_tx.send(BackendEvent::DeviceListChanged);
        }
    }

    /// Add a device to the list and emit `DeviceListChanged`.
    pub fn add_device(&mut self, device: FakeDevice) {
        self.devices.push(device);
        let _ = self.events_tx.send(BackendEvent::DeviceListChanged);
    }

    /// Change a listed device's reported rate and emit `SampleRateChanged`.
    pub fn set_rate(&mut self, id: &DeviceId, rate: SampleRate) {
        if let Some(device) = self.devices.iter_mut().find(|d| &d.id == id) {
            device.rate = rate;
        }
        let _ = self.events_tx.send(BackendEvent::SampleRateChanged {
            id: id.clone(),
            new_rate: rate,
        });
    }

    /// Change which listed device reports `is_default: true`.
    pub fn set_default(&mut self, id: &DeviceId) {
        for device in &mut self.devices {
            device.is_default = &device.id == id;
        }
    }

    /// Take the active stream's processor, detaching it from this backend
    /// without emitting any event. Lets a test inspect (or rebuild from)
    /// the processor after the stream it belonged to is gone.
    pub fn take_processor(&mut self) -> Option<Processor<SyntheticSource>> {
        self.active.take().map(|a| a.processor)
    }
}

impl OutputBackend for FakeBackend {
    fn devices(&self) -> Result<Vec<OutputDeviceInfo>, AudioIoError> {
        Ok(self.devices.iter().map(FakeDevice::to_info).collect())
    }

    fn default_device(&self) -> Result<Option<OutputDeviceInfo>, AudioIoError> {
        Ok(self
            .devices
            .iter()
            .find(|d| d.is_default)
            .map(FakeDevice::to_info))
    }

    fn open(
        &mut self,
        device: &DeviceId,
        preset: BufferPreset,
        processor: Processor<SyntheticSource>,
    ) -> Result<OpenStream, AudioIoError> {
        let found = self
            .devices
            .iter()
            .find(|d| &d.id == device)
            .cloned()
            .ok_or_else(|| AudioIoError::NoSuchDevice(device.clone()))?;

        let requested = preset.requested_frames().frames();
        let clamped = match found.buffer_range {
            Some((min, max)) => requested.clamp(min.frames(), max.frames()),
            None => requested,
        };
        let negotiated = NegotiatedBuffer {
            preset,
            frames: FrameCount::new(clamped),
            device_rate: found.rate,
        };

        self.active = Some(ActiveStream {
            device_id: found.id.clone(),
            device_channels: found.channels,
            negotiated,
            processor,
        });

        Ok(OpenStream::new(negotiated, Box::new(FakeStreamHandle)))
    }

    fn events(&self) -> &Receiver<BackendEvent> {
        &self.events_rx
    }
}

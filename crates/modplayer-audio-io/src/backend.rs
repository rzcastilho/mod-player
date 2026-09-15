// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `OutputBackend` trait and its supporting types
//! (contracts/output-backend.md, data-model.md §4). This crate is the only
//! one that knows a platform audio API; the engine never sees `cpal` types.
//! Two implementors: `CpalBackend` (production, US1 T046) and `FakeBackend`
//! (tests, T038).

use std::sync::mpsc::Receiver;

use modplayer_audio_source_synthetic::SyntheticSource;
use modplayer_engine::{
    BufferPreset, DeviceId, FrameCount, NegotiatedBuffer, Processor, SampleRate,
};

use crate::error::AudioIoError;

/// A snapshot of one enumerated output-capable device.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputDeviceInfo {
    pub id: DeviceId,
    pub name: String,
    pub is_default: bool,
    pub default_rate: SampleRate,
    pub channels: u16,
    pub buffer_range: Option<(FrameCount, FrameCount)>,
}

/// The inputs to opening a stream (data-model.md §4). `OutputBackend::open`
/// takes these as separate parameters rather than this struct
/// (contracts/output-backend.md); it is kept as a named type for
/// documentation and for callers that want to build a request ahead of
/// time.
pub struct StreamRequest {
    pub device: DeviceId,
    pub preset: BufferPreset,
    pub processor: Processor<SyntheticSource>,
}

/// Backend-specific handle to an open stream. Dropping it stops and
/// releases the stream — there is no other way to close one.
pub trait StreamHandle: Send {}

/// A successfully opened output stream.
pub struct OpenStream {
    /// The buffer actually negotiated with the device for this stream.
    pub negotiated: NegotiatedBuffer,
    #[allow(dead_code)]
    handle: Box<dyn StreamHandle>,
}

impl OpenStream {
    /// Construct an `OpenStream` from its negotiated buffer and a
    /// backend-specific handle. For use by `OutputBackend` implementors.
    pub fn new(negotiated: NegotiatedBuffer, handle: Box<dyn StreamHandle>) -> Self {
        Self { negotiated, handle }
    }
}

/// Asynchronous device events, delivered off the real-time path
/// (data-model.md §4).
#[derive(Debug, Clone, PartialEq)]
pub enum BackendEvent {
    /// The active stream's device disappeared.
    DeviceLost { id: DeviceId },
    /// Any device add/remove; the controller re-enumerates.
    DeviceListChanged,
    /// The active device's sample rate changed externally.
    SampleRateChanged { id: DeviceId, new_rate: SampleRate },
}

/// The only trait that knows a platform audio API. Implementations:
/// `CpalBackend` (production) and `FakeBackend` (tests, no platform
/// dependencies).
pub trait OutputBackend: Send {
    /// Enumerate output-capable devices. Never called on the real-time path.
    fn devices(&self) -> Result<Vec<OutputDeviceInfo>, AudioIoError>;

    /// The host's current default output device, if any.
    fn default_device(&self) -> Result<Option<OutputDeviceInfo>, AudioIoError>;

    /// Open a stream on `device` requesting `preset.requested_frames()`
    /// frames per callback, at the device's current default sample rate,
    /// `f32` samples, the device's channel count. The backend clamps the
    /// request into the device's supported buffer range and reports the
    /// result in `OpenStream::negotiated`. `processor` is moved into the
    /// callback and driven with `Processor::render` for every buffer until
    /// the stream is dropped.
    fn open(
        &mut self,
        device: &DeviceId,
        preset: BufferPreset,
        processor: Processor<SyntheticSource>,
    ) -> Result<OpenStream, AudioIoError>;

    /// Receiver for asynchronous device events, delivered off the
    /// real-time path.
    fn events(&self) -> &Receiver<BackendEvent>;
}

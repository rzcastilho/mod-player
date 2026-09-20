// SPDX-License-Identifier: MIT OR Apache-2.0

//! `CpalBackend`: the production `OutputBackend`, and the only module in
//! ModPlayer that imports `cpal` (contracts/output-backend.md). `cpal`'s API
//! is entirely safe, so this module needs no `unsafe` (Constitution
//! Principle I/VII: `#![forbid(unsafe_code)]` holds workspace-wide).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize as CpalBufferSize, FromSample, SampleFormat, SizedSample, StreamConfig};
use modplayer_audio_source::AudioSource;
use modplayer_engine::{
    BufferPreset, DeviceId, FrameCount, NegotiatedBuffer, Processor, RtShared, SampleRate,
};

use crate::backend::{BackendEvent, OpenStream, OutputBackend, OutputDeviceInfo, StreamHandle};
use crate::error::AudioIoError;
use crate::watcher::DeviceWatcher;

/// Upper bound on interleaved samples converted per `render` call on a
/// non-F32 device (arithmetic-only conversion path). Real device callbacks
/// are always far smaller than this in practice (buffer presets top out at
/// 1024 frames); when a callback somehow delivers more, it is processed in
/// several bounded chunks rather than allocating.
const CONVERT_SCRATCH_FRAMES: usize = 4096;

/// Marker handle: dropping it stops and releases the `cpal::Stream`
/// (`Stream`'s own `Drop` impl does this).
struct CpalStreamHandle(#[allow(dead_code)] cpal::Stream);

impl StreamHandle for CpalStreamHandle {}

/// The production `OutputBackend`, backed by real hardware via `cpal`.
pub struct CpalBackend {
    host: cpal::Host,
    events_tx: Sender<BackendEvent>,
    events_rx: Receiver<BackendEvent>,
    /// The watcher thread for whichever device is currently open, if any
    /// (US3 T069-T070). Replaced (dropping, and so stopping, the previous
    /// one) each time `open` succeeds; `None` before the first `open`.
    watcher: Option<DeviceWatcher>,
}

impl CpalBackend {
    /// Construct a backend on `cpal`'s default host for this platform.
    pub fn new() -> Self {
        let (events_tx, events_rx) = channel();
        Self {
            host: cpal::default_host(),
            events_tx,
            events_rx,
            watcher: None,
        }
    }

    fn find_device(&self, id: &DeviceId) -> Result<cpal::Device, AudioIoError> {
        self.host
            .output_devices()
            .map_err(|e| AudioIoError::Backend(e.to_string()))?
            .find(|d| device_id_of(d) == id.as_str())
            .ok_or_else(|| AudioIoError::NoSuchDevice(id.clone()))
    }
}

impl Default for CpalBackend {
    fn default() -> Self {
        Self::new()
    }
}

/// The stable id for a `cpal` device: its own `DeviceId` display form, or a
/// `name:<name>` fallback if `id()` errors (contracts/output-backend.md).
fn device_id_of(device: &cpal::Device) -> String {
    match device.id() {
        Ok(id) => id.to_string(),
        Err(_) => format!("name:{device}"),
    }
}

/// Whether `device`'s stable id (contracts/output-backend.md) equals `id`.
/// Shared with `watcher.rs` so the watcher thread can find the currently
/// active device again on each poll without duplicating this rule.
pub(crate) fn device_id_matches(device: &cpal::Device, id: &DeviceId) -> bool {
    device_id_of(device) == id.as_str()
}

fn device_info(device: &cpal::Device, default_id: Option<&str>) -> Option<OutputDeviceInfo> {
    let config = device.default_output_config().ok()?;
    let id_str = device_id_of(device);
    let id = DeviceId::new(id_str.clone())?;
    let buffer_range = match *config.buffer_size() {
        cpal::SupportedBufferSize::Range { min, max } => {
            Some((FrameCount::new(min), FrameCount::new(max)))
        }
        cpal::SupportedBufferSize::Unknown => None,
    };
    Some(OutputDeviceInfo {
        id,
        name: device.to_string(),
        is_default: default_id == Some(id_str.as_str()),
        default_rate: SampleRate::new(config.sample_rate()),
        channels: config.channels(),
        buffer_range,
    })
}

/// Build and start an output stream of sample type `T`, converting the
/// engine's `f32` mix arithmetically when `T != f32` (contracts:
/// "F32 preferred; otherwise convert ... arithmetic only").
fn build_and_play<T, S>(
    device: &cpal::Device,
    stream_config: &StreamConfig,
    device_channels: u16,
    mut processor: Processor<S>,
    shared: Arc<RtShared>,
    events_tx: Sender<BackendEvent>,
    lost_id: DeviceId,
) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample + FromSample<f32>,
    S: AudioSource,
{
    let first_callback = AtomicBool::new(true);
    let channels = usize::from(device_channels).max(1);
    let mut convert_scratch = vec![0.0f32; CONVERT_SCRATCH_FRAMES * channels];

    let stream = device.build_output_stream(
        *stream_config,
        move |data: &mut [T], _info: &cpal::OutputCallbackInfo| {
            if first_callback.swap(false, Ordering::Relaxed) {
                shared.set_negotiated_frames((data.len() / channels) as u32);
            }
            for chunk in data.chunks_mut(convert_scratch.len()) {
                let scratch = &mut convert_scratch[..chunk.len()];
                processor.render(scratch);
                for (out, &sample) in chunk.iter_mut().zip(scratch.iter()) {
                    *out = T::from_sample(sample);
                }
            }
        },
        move |err| {
            if err.kind() == cpal::ErrorKind::DeviceNotAvailable {
                let _ = events_tx.send(BackendEvent::DeviceLost {
                    id: lost_id.clone(),
                });
            }
        },
        None,
    )?;
    stream.play()?;
    Ok(stream)
}

/// The F32 direct path: no conversion, `Processor::render` writes straight
/// into the device's callback buffer.
fn build_and_play_f32<S: AudioSource>(
    device: &cpal::Device,
    stream_config: &StreamConfig,
    device_channels: u16,
    mut processor: Processor<S>,
    shared: Arc<RtShared>,
    events_tx: Sender<BackendEvent>,
    lost_id: DeviceId,
) -> Result<cpal::Stream, cpal::Error> {
    let first_callback = AtomicBool::new(true);
    let channels = usize::from(device_channels).max(1);

    let stream = device.build_output_stream(
        *stream_config,
        move |data: &mut [f32], _info: &cpal::OutputCallbackInfo| {
            if first_callback.swap(false, Ordering::Relaxed) {
                shared.set_negotiated_frames((data.len() / channels) as u32);
            }
            processor.render(data);
        },
        move |err| {
            if err.kind() == cpal::ErrorKind::DeviceNotAvailable {
                let _ = events_tx.send(BackendEvent::DeviceLost {
                    id: lost_id.clone(),
                });
            }
        },
        None,
    )?;
    stream.play()?;
    Ok(stream)
}

impl OutputBackend for CpalBackend {
    fn devices(&self) -> Result<Vec<OutputDeviceInfo>, AudioIoError> {
        let default_id = self.host.default_output_device().map(|d| device_id_of(&d));
        let devices = self
            .host
            .output_devices()
            .map_err(|e| AudioIoError::Backend(e.to_string()))?;
        Ok(devices
            .filter_map(|d| device_info(&d, default_id.as_deref()))
            .collect())
    }

    fn default_device(&self) -> Result<Option<OutputDeviceInfo>, AudioIoError> {
        Ok(self.host.default_output_device().and_then(|d| {
            let id = device_id_of(&d);
            device_info(&d, Some(&id))
        }))
    }

    fn open<S: AudioSource>(
        &mut self,
        device_id: &DeviceId,
        preset: BufferPreset,
        processor: Processor<S>,
    ) -> Result<OpenStream, AudioIoError> {
        let device = self.find_device(device_id)?;
        let default_config =
            device
                .default_output_config()
                .map_err(|e| AudioIoError::UnsupportedConfig {
                    device: device_id.clone(),
                    reason: e.to_string(),
                })?;

        let requested = preset.requested_frames().frames();
        let (buffer_size, negotiated_hint) = match *default_config.buffer_size() {
            cpal::SupportedBufferSize::Range { min, max } => {
                let clamped = requested.clamp(min, max);
                (CpalBufferSize::Fixed(clamped), clamped)
            }
            cpal::SupportedBufferSize::Unknown => (CpalBufferSize::Default, requested),
        };

        let stream_config = StreamConfig {
            channels: default_config.channels(),
            sample_rate: default_config.sample_rate(),
            buffer_size,
        };
        let device_channels = default_config.channels();
        let device_rate = default_config.sample_rate();
        let shared = processor.shared();
        let events_tx = self.events_tx.clone();
        let lost_id = device_id.clone();

        let stream = match default_config.sample_format() {
            SampleFormat::F32 => build_and_play_f32(
                &device,
                &stream_config,
                device_channels,
                processor,
                shared,
                events_tx,
                lost_id,
            ),
            SampleFormat::I8 => build_and_play::<i8, S>(
                &device,
                &stream_config,
                device_channels,
                processor,
                shared,
                events_tx,
                lost_id,
            ),
            SampleFormat::I16 => build_and_play::<i16, S>(
                &device,
                &stream_config,
                device_channels,
                processor,
                shared,
                events_tx,
                lost_id,
            ),
            SampleFormat::I32 => build_and_play::<i32, S>(
                &device,
                &stream_config,
                device_channels,
                processor,
                shared,
                events_tx,
                lost_id,
            ),
            SampleFormat::I64 => build_and_play::<i64, S>(
                &device,
                &stream_config,
                device_channels,
                processor,
                shared,
                events_tx,
                lost_id,
            ),
            SampleFormat::U8 => build_and_play::<u8, S>(
                &device,
                &stream_config,
                device_channels,
                processor,
                shared,
                events_tx,
                lost_id,
            ),
            SampleFormat::U16 => build_and_play::<u16, S>(
                &device,
                &stream_config,
                device_channels,
                processor,
                shared,
                events_tx,
                lost_id,
            ),
            SampleFormat::U32 => build_and_play::<u32, S>(
                &device,
                &stream_config,
                device_channels,
                processor,
                shared,
                events_tx,
                lost_id,
            ),
            SampleFormat::U64 => build_and_play::<u64, S>(
                &device,
                &stream_config,
                device_channels,
                processor,
                shared,
                events_tx,
                lost_id,
            ),
            SampleFormat::F64 => build_and_play::<f64, S>(
                &device,
                &stream_config,
                device_channels,
                processor,
                shared,
                events_tx,
                lost_id,
            ),
            other => {
                return Err(AudioIoError::UnsupportedConfig {
                    device: device_id.clone(),
                    reason: format!("sample format {other} has no arithmetic f32 conversion"),
                });
            }
        }
        .map_err(|e| AudioIoError::Backend(e.to_string()))?;

        let negotiated = NegotiatedBuffer {
            preset,
            frames: FrameCount::new(negotiated_hint),
            device_rate: SampleRate::new(device_rate),
        };

        // Replace any previous stream's watcher with one for the newly
        // opened device (dropping the old one stops its thread).
        self.watcher = Some(DeviceWatcher::spawn(
            device_id.clone(),
            self.events_tx.clone(),
        ));

        Ok(OpenStream::new(
            negotiated,
            Box::new(CpalStreamHandle(stream)),
        ))
    }

    fn events(&self) -> &Receiver<BackendEvent> {
        &self.events_rx
    }
}

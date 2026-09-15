# Contract: `OutputBackend` trait and backend events

**Crate**: `modplayer-audio-io` | **Implementors**: `CpalBackend` (production), `FakeBackend` (tests) | **Traces**: FR-001, FR-004, FR-005, FR-012, FR-013, FR-023, FR-024

Two implementors justify the trait under Constitution X. The backend is the only crate that knows the platform audio API; the engine never sees cpal types.

```rust
pub trait OutputBackend: Send {
    /// Enumerate output-capable devices. Never called on the real-time path.
    fn devices(&self) -> Result<Vec<OutputDeviceInfo>, AudioIoError>;

    /// The host's current default output device, if any.
    fn default_device(&self) -> Result<Option<OutputDeviceInfo>, AudioIoError>;

    /// Open a stream on `device` requesting `preset.requested_frames()` frames per callback,
    /// at the device's current default sample rate, f32 samples, the device's channel count.
    /// The backend clamps the request into the device's supported buffer range and reports
    /// the result in `OpenStream::negotiated`. `processor` is moved into the callback and
    /// driven with `Processor::render` for every buffer until the stream is dropped.
    fn open(
        &mut self,
        device: &DeviceId,
        preset: BufferPreset,
        processor: Processor<SyntheticSource>,
    ) -> Result<OpenStream, AudioIoError>;

    /// Receiver for asynchronous device events, delivered off the real-time path.
    fn events(&self) -> &std::sync::mpsc::Receiver<BackendEvent>;
}

pub struct OpenStream {
    pub negotiated: NegotiatedBuffer,   // frames actually delivered, device rate, preset
    handle: Box<dyn StreamHandle>,      // dropping stops and releases the stream
}

pub enum BackendEvent {
    DeviceLost { id: DeviceId },              // active stream's device disappeared
    DeviceListChanged,                        // any add/remove; controller re-enumerates
    SampleRateChanged { id: DeviceId, new_rate: SampleRate },
}

pub enum AudioIoError {
    NoSuchDevice(DeviceId),
    NoOutputDevices,
    DeviceUnavailable(DeviceId),
    UnsupportedConfig { device: DeviceId, reason: String },
    Backend(String),
}
```

## `CpalBackend` behaviour

| Concern | Behaviour |
|---|---|
| Device identity | `DeviceId` = `cpal::DeviceId` display string; if `id()` errors, `name:<device name>` (FR-004). Lookup on launch: by id, then by name |
| Sample format | Prefer `F32`; otherwise convert from the device's format inside the callback via cpal `Sample` conversions (arithmetic only) |
| Buffer size | `BufferSize::Fixed(clamp(requested, min, max))` when the device reports a range; `BufferSize::Default` when `Unknown`. Negotiated frames = first callback's `data.len()/channels`, also stored in `RtShared::negotiated_frames` |
| Device loss | cpal error callback with `ErrorKind::DeviceNotAvailable` → `BackendEvent::DeviceLost` on the mpsc channel. The error callback runs off the audio callback; it may allocate |
| Sample-rate change | A watcher thread polls the active device's `default_output_config()` every 500 ms (off real-time path); a differing rate → `SampleRateChanged` |
| Device list | Same watcher thread re-enumerates every 2 s and emits `DeviceListChanged` on any difference (reappearance detection, FR-012) |
| Channels | Stream opened with the device's default channel count; `Processor` maps stereo → N per FR-023 |
| Callback | Calls `processor.render(data)`; on any panic-free error path it writes silence. No logging inside |

## `FakeBackend` behaviour (tests only, no platform dependencies)

| Method | Behaviour |
|---|---|
| `FakeBackend::new(devices: Vec<FakeDevice { id, name, rate, channels, buffer_range }>)` | scripted device list; first device with `is_default` is the default |
| `open` | stores the processor; `negotiated.frames` = clamp(request, range) |
| `render_buffers(n) -> Vec<f32>` | drives `render` `n` times synchronously with the negotiated buffer size, returns concatenated output (interleaved, device channels) |
| `remove_device(id)` | removes from list; if active, drops its stream state and emits `DeviceLost` then `DeviceListChanged` |
| `add_device(dev)` | emits `DeviceListChanged` |
| `set_rate(id, rate)` | emits `SampleRateChanged` |
| `set_default(id)` | changes the reported default |
| `take_processor() -> Option<Processor>` | lets a test inspect the processor after dropping a stream (used by FR-025(c)) |

## Tests that pin this contract

- `modplayer-audio-io`: `FakeBackend` unit tests for every method above; a `CpalBackend` smoke test (`#[ignore = "manual: needs an audio device"]`) that enumerates, opens the default device at each preset, renders silence for 1 s, and prints the negotiated frames.
- `modplayer-core` (integration, `FakeBackend`): `fallback_on_device_lost`, `pause_when_no_device_remains`, `no_switch_back_on_reappear`, `missing_preferred_at_launch_uses_default_with_warning`, `rate_change_rebuilds_output_stage_only`, `zero_devices_at_launch`.

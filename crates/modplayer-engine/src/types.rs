// SPDX-License-Identifier: MIT OR Apache-2.0

//! Value types shared across the real-time engine and its callers.
//!
//! Every type here that carries an invariant (a range, a non-empty string,
//! a fixed step) is a newtype whose constructor clamps or validates, so
//! there is no way to hold an out-of-range value once constructed
//! (data-model.md §1).

use std::time::Duration;

/// Sample rate in Hz. Always > 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SampleRate(u32);

impl SampleRate {
    /// Construct a sample rate, clamping zero up to 1 Hz (a zero rate has no
    /// sensible meaning and must never be constructible).
    pub fn new(hz: u32) -> Self {
        Self(hz.max(1))
    }

    /// The sample rate in Hz.
    pub fn hz(self) -> u32 {
        self.0
    }
}

impl Default for SampleRate {
    fn default() -> Self {
        Self::new(44_100)
    }
}

/// A count of audio frames. Always > 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FrameCount(u32);

impl FrameCount {
    /// Construct a frame count, clamping zero up to 1 frame.
    pub fn new(frames: u32) -> Self {
        Self(frames.max(1))
    }

    /// The number of frames.
    pub fn frames(self) -> u32 {
        self.0
    }
}

/// The three user-facing buffer-size presets (FR-005).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BufferPreset {
    Performance,
    #[default]
    Balanced,
    Safe,
}

impl BufferPreset {
    /// Frames requested per callback for this preset.
    pub fn requested_frames(self) -> FrameCount {
        FrameCount::new(match self {
            BufferPreset::Performance => 128,
            BufferPreset::Balanced => 256,
            BufferPreset::Safe => 1024,
        })
    }
}

/// The buffer size actually negotiated with a device for a given preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NegotiatedBuffer {
    pub preset: BufferPreset,
    pub frames: FrameCount,
    pub device_rate: SampleRate,
}

impl NegotiatedBuffer {
    /// Latency in milliseconds implied by `frames` at `device_rate`.
    pub fn latency_ms(&self) -> f64 {
        f64::from(self.frames.frames()) / f64::from(self.device_rate.hz()) * 1000.0
    }

    /// "One buffer duration" as a `Duration`, used as the bound for device
    /// fallback and rate-change gaps (FR-012, FR-013).
    pub fn duration(&self) -> Duration {
        Duration::from_secs_f64(f64::from(self.frames.frames()) / f64::from(self.device_rate.hz()))
    }
}

/// The limiter ceiling in dBFS. Clamped to `[-6.0, -0.1]` and rounded to a
/// 0.1 dB step (FR-010).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CeilingDb(f32);

impl CeilingDb {
    pub const MIN: f32 = -6.0;
    pub const MAX: f32 = -0.1;
    const STEP: f32 = 0.1;

    /// Construct a ceiling, clamping into range and snapping to the nearest
    /// 0.1 dB step. Clamped again after rounding to absorb float error at
    /// the boundary.
    pub fn new(db: f32) -> Self {
        let clamped = db.clamp(Self::MIN, Self::MAX);
        let stepped = (clamped / Self::STEP).round() * Self::STEP;
        Self(stepped.clamp(Self::MIN, Self::MAX))
    }

    /// Construct from a settings-file `f64`, clamping the same way.
    pub fn from_f64(db: f64) -> Self {
        Self::new(db as f32)
    }

    /// The ceiling in dBFS.
    pub fn db(self) -> f32 {
        self.0
    }

    /// The ceiling as a linear amplitude (`10^(db/20)`).
    pub fn to_linear(self) -> f32 {
        10f32.powf(self.0 / 20.0)
    }
}

impl Default for CeilingDb {
    fn default() -> Self {
        Self::new(-1.0)
    }
}

/// Master/cap volume as a 0-100 percentage (FR-011).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct VolumePercent(u8);

impl VolumePercent {
    /// Construct from a `u8`, clamping to 100.
    pub fn new(pct: u8) -> Self {
        Self(pct.min(100))
    }

    /// Construct from an arbitrary signed settings-file integer, clamping
    /// into `0..=100`.
    pub fn from_i64(pct: i64) -> Self {
        Self(pct.clamp(0, 100) as u8)
    }

    /// The percentage, `0..=100`.
    pub fn value(self) -> u8 {
        self.0
    }

    /// Linear gain, `0.0..=1.0` (100% = unity).
    pub fn to_linear(self) -> f32 {
        f32::from(self.0) / 100.0
    }

    /// Display value in dB; `-inf` at 0%.
    pub fn to_db(self) -> f32 {
        if self.0 == 0 {
            f32::NEG_INFINITY
        } else {
            20.0 * self.to_linear().log10()
        }
    }
}

impl Default for VolumePercent {
    fn default() -> Self {
        Self::new(80)
    }
}

/// The safe-volume startup cap (FR-011).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SafeVolume {
    pub enabled: bool,
    pub cap: VolumePercent,
}

impl SafeVolume {
    /// Apply the cap to a stored volume: clamps down to `cap` when enabled,
    /// passes `stored` through unchanged otherwise.
    pub fn apply(self, stored: VolumePercent) -> VolumePercent {
        if self.enabled {
            VolumePercent::new(stored.value().min(self.cap.value()))
        } else {
            stored
        }
    }
}

impl Default for SafeVolume {
    fn default() -> Self {
        Self {
            enabled: true,
            cap: VolumePercent::new(50),
        }
    }
}

/// Transport state (FR-015).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Transport {
    #[default]
    Stopped,
    Playing,
    Paused,
}

/// A stable output-device identifier: the backend's `DeviceId` display
/// form, or a `name:<name>` fallback. Always non-empty.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DeviceId(String);

impl DeviceId {
    /// Construct a device id. Returns `None` for an empty string, which
    /// carries no invariant-respecting meaning (FR-004).
    pub fn new(id: impl Into<String>) -> Option<Self> {
        let id = id.into();
        if id.is_empty() { None } else { Some(Self(id)) }
    }

    /// The id as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for DeviceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Theme preference (FR-017).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_preset_frames_and_default() {
        assert_eq!(BufferPreset::Performance.requested_frames().frames(), 128);
        assert_eq!(BufferPreset::Balanced.requested_frames().frames(), 256);
        assert_eq!(BufferPreset::Safe.requested_frames().frames(), 1024);
        assert_eq!(BufferPreset::default(), BufferPreset::Balanced);
    }

    #[test]
    fn ceiling_clamps_and_steps() {
        assert_eq!(CeilingDb::new(0.0).db(), -0.1);
        assert_eq!(CeilingDb::new(-20.0).db(), -6.0);
        assert_eq!(CeilingDb::default().db(), -1.0);
    }

    #[test]
    fn volume_clamps() {
        assert_eq!(VolumePercent::new(150).value(), 100);
        assert_eq!(VolumePercent::from_i64(250).value(), 100);
        assert_eq!(VolumePercent::from_i64(-5).value(), 0);
    }

    #[test]
    fn safe_volume_applies_cap() {
        let safe = SafeVolume {
            enabled: true,
            cap: VolumePercent::new(50),
        };
        assert_eq!(safe.apply(VolumePercent::new(80)).value(), 50);
        assert_eq!(safe.apply(VolumePercent::new(30)).value(), 30);
        let disabled = SafeVolume {
            enabled: false,
            cap: VolumePercent::new(50),
        };
        assert_eq!(disabled.apply(VolumePercent::new(80)).value(), 80);
    }

    #[test]
    fn device_id_rejects_empty() {
        assert!(DeviceId::new("").is_none());
        assert_eq!(
            DeviceId::new("abc").as_ref().map(DeviceId::as_str),
            Some("abc")
        );
    }
}

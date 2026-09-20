// SPDX-License-Identifier: MIT OR Apache-2.0

//! `HostMixer`: librespot's `Mixer`, backed by the host's own volume
//! (data-model.md §5). No audio effect here — the engine applies the
//! actual gain (Constitution V: decoded audio never leaves the engine);
//! this only tracks the value Spirc reports/reads, in librespot's `u16`
//! scale, with echo suppression so a host-initiated `SetVolume` does not
//! bounce back as a `RemoteCommand::Volume`.

use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};

use librespot_core::Error;
use librespot_playback::mixer::{Mixer, MixerConfig, NoOpVolume, VolumeGetter};

/// A `VolumePercent` (0-100) mapped to librespot's `u16` (0-65535) scale
/// and back, round-tripping within ±1 % (contracts/connect-source.md §7).
pub fn to_u16(pct: u8) -> u16 {
    ((u32::from(pct.min(100)) * u32::from(u16::MAX)) / 100) as u16
}

pub fn to_pct(volume: u16) -> u8 {
    ((u32::from(volume) * 100 + u32::from(u16::MAX) / 2) / u32::from(u16::MAX)) as u8
}

/// Host-driven `Mixer`. `set_volume` is what Spirc calls both when the
/// host asks it to (`HostMixer::set_from_host`, which arms
/// `suppress_echo` first) and when a *remote* controller changes volume —
/// the worker distinguishes the two by checking `take_echo()` immediately
/// after the call that triggered it returns.
pub struct HostMixer {
    volume: AtomicU16,
    suppress_echo: AtomicBool,
}

impl HostMixer {
    pub fn new(initial_pct: u8) -> Self {
        Self {
            volume: AtomicU16::new(to_u16(initial_pct)),
            suppress_echo: AtomicBool::new(false),
        }
    }

    /// The host is setting the volume (`SourceCommand::SetVolume`): arm
    /// echo suppression so the resulting `VolumeChanged` player event is
    /// not re-reported as a `RemoteCommand`.
    pub fn set_from_host(&self, pct: u8) {
        self.suppress_echo.store(true, Ordering::Release);
        self.volume.store(to_u16(pct), Ordering::Release);
    }

    /// Consume the echo-suppression flag: `true` at most once per
    /// `set_from_host` call (contracts/connect-source.md §3).
    pub fn take_echo(&self) -> bool {
        self.suppress_echo.swap(false, Ordering::AcqRel)
    }

    pub fn volume_pct(&self) -> u8 {
        to_pct(self.volume.load(Ordering::Acquire))
    }
}

impl Mixer for HostMixer {
    fn open(_config: MixerConfig) -> Result<Self, Error>
    where
        Self: Sized,
    {
        // Never actually called: `ConnectSource` constructs `HostMixer`
        // directly and hands `Spirc::new` an `Arc<dyn Mixer>` — librespot's
        // `mixer::find`/`MIXERS` registry (which calls `open`) is unused.
        Ok(Self::new(50))
    }

    fn volume(&self) -> u16 {
        self.volume.load(Ordering::Acquire)
    }

    fn set_volume(&self, volume: u16) {
        self.volume.store(volume, Ordering::Release);
    }

    fn get_soft_volume(&self) -> Box<dyn VolumeGetter + Send> {
        // The engine applies gain (Constitution V); librespot's own
        // software attenuation stays a no-op.
        Box::new(NoOpVolume)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_round_trips_within_one_percent() {
        for pct in 0..=100u8 {
            let back = to_pct(to_u16(pct));
            let diff = i16::from(back) - i16::from(pct);
            assert!(diff.abs() <= 1, "{pct} -> {back} (diff {diff})");
        }
    }

    #[test]
    fn zero_and_max_map_to_the_scale_ends() {
        assert_eq!(to_u16(0), 0);
        assert_eq!(to_u16(100), u16::MAX);
        assert_eq!(to_pct(0), 0);
        assert_eq!(to_pct(u16::MAX), 100);
    }

    #[test]
    fn echo_is_armed_by_set_from_host_and_consumed_once() {
        let mixer = HostMixer::new(50);
        assert!(!mixer.take_echo());
        mixer.set_from_host(80);
        assert!(mixer.take_echo());
        assert!(!mixer.take_echo(), "echo flag must be consumed once");
        assert_eq!(mixer.volume_pct(), 80);
    }

    #[test]
    fn remote_set_volume_does_not_arm_echo() {
        let mixer = HostMixer::new(50);
        mixer.set_volume(to_u16(30));
        assert!(!mixer.take_echo());
        assert_eq!(mixer.volume_pct(), 30);
    }
}

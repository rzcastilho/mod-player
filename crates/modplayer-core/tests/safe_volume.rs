// SPDX-License-Identifier: MIT OR Apache-2.0

//! Safe-volume startup clamp tests (spec US2 acceptance 2, 3; SC-009): the
//! effective master volume at launch is `SafeVolume::apply(stored)`
//! (data-model.md §1, `PlaybackController` construction). Raising the
//! volume above the cap during a session applies and persists the raw
//! value; the next launch re-clamps it. Disabled safe volume leaves the
//! stored value unchanged.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use modplayer_audio_io::FakeBackend;
use modplayer_audio_source_synthetic::SyntheticHost;
use modplayer_core::PlaybackController;
use modplayer_core::settings::{AudioSettings, SettingsStore};
use modplayer_engine::{SafeVolume, VolumePercent};

/// A minimal self-cleaning temp directory (no `tempfile` dependency),
/// mirroring `tests/settings.rs` / `tests/device_policy.rs`'s isolation
/// approach.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-safe-volume-test-{}-{}",
            std::process::id(),
            unique
        ));
        let _ = std::fs::create_dir_all(&dir);
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn store_with(settings: &AudioSettings) -> (SettingsStore, TempDir) {
    let dir = TempDir::new();
    let store = SettingsStore::with_path(dir.path().join("settings.toml"));
    assert!(store.save(settings).is_ok(), "seed settings file");
    (store, dir)
}

#[test]
fn enabled_cap_clamps_stored_volume_above_cap_at_launch() {
    let settings = AudioSettings {
        safe_volume: SafeVolume {
            enabled: true,
            cap: VolumePercent::new(50),
        },
        master_volume: VolumePercent::new(80), // stored V (80) > cap C (50)
        ..AudioSettings::default()
    };
    let (store, _dir) = store_with(&settings);

    let controller =
        PlaybackController::new(FakeBackend::new(vec![]), SyntheticHost::new(44_100), store);

    assert_eq!(
        controller.master_volume(),
        VolumePercent::new(50),
        "effective volume at launch must be min(stored, cap) = the cap"
    );
}

#[test]
fn enabled_cap_leaves_stored_volume_at_or_below_cap_unchanged() {
    let settings = AudioSettings {
        safe_volume: SafeVolume {
            enabled: true,
            cap: VolumePercent::new(50),
        },
        master_volume: VolumePercent::new(30), // stored V (30) <= cap C (50)
        ..AudioSettings::default()
    };
    let (store, _dir) = store_with(&settings);

    let controller =
        PlaybackController::new(FakeBackend::new(vec![]), SyntheticHost::new(44_100), store);

    assert_eq!(controller.master_volume(), VolumePercent::new(30));
}

#[test]
fn raising_above_the_cap_applies_and_persists_then_reclamps_next_launch() {
    let settings = AudioSettings {
        safe_volume: SafeVolume {
            enabled: true,
            cap: VolumePercent::new(50),
        },
        master_volume: VolumePercent::new(30),
        ..AudioSettings::default()
    };
    let (store, _dir) = store_with(&settings);

    let mut controller = PlaybackController::new(
        FakeBackend::new(vec![]),
        SyntheticHost::new(44_100),
        store.clone(),
    );
    assert_eq!(controller.master_volume(), VolumePercent::new(30));

    // Raise above the cap: applied immediately this session...
    controller.set_master_volume(VolumePercent::new(90));
    assert_eq!(controller.master_volume(), VolumePercent::new(90));

    // ...and persisted as the raw (uncapped) value.
    let persisted = store.load().settings;
    assert_eq!(persisted.master_volume, VolumePercent::new(90));

    // Next launch: safe volume re-clamps the persisted value to the cap again.
    let relaunched =
        PlaybackController::new(FakeBackend::new(vec![]), SyntheticHost::new(44_100), store);
    assert_eq!(relaunched.master_volume(), VolumePercent::new(50));
}

#[test]
fn disabled_uses_the_saved_value_unchanged() {
    let settings = AudioSettings {
        safe_volume: SafeVolume {
            enabled: false,
            cap: VolumePercent::new(50),
        },
        master_volume: VolumePercent::new(90),
        ..AudioSettings::default()
    };
    let (store, _dir) = store_with(&settings);

    let controller =
        PlaybackController::new(FakeBackend::new(vec![]), SyntheticHost::new(44_100), store);

    assert_eq!(
        controller.master_volume(),
        VolumePercent::new(90),
        "safe volume disabled: launch volume is the saved value, unchanged"
    );
}

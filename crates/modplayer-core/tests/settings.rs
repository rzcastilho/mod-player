// SPDX-License-Identifier: MIT OR Apache-2.0

//! Settings-file contract tests (FR-020, FR-025e; contracts/settings-file.md):
//! round-trip, out-of-range clamping, a garbage file, a simulated crash
//! mid-write, an unknown key, and a newer schema version. Exercises only
//! `SettingsStore`'s public API, each test against its own directory under
//! `MODPLAYER_CONFIG_DIR`-style isolation via `SettingsStore::with_path` (no
//! test touches the real platform config location).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use modplayer_core::settings::{
    AudioSettings, DisclosureAcknowledgement, SettingsStore, SettingsWarning,
};
use modplayer_engine::{BufferPreset, VolumePercent};

/// A minimal self-cleaning temp directory (no `tempfile` dependency).
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-settings-integration-{}-{}",
            std::process::id(),
            unique
        ));
        let _ = fs::create_dir_all(&dir);
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn store_in(dir: &TempDir) -> SettingsStore {
    SettingsStore::with_path(dir.path().join("settings.toml"))
}

#[test]
fn round_trip_defaults() {
    let dir = TempDir::new();
    let store = store_in(&dir);
    let settings = AudioSettings::default();
    assert!(store.save(&settings).is_ok());

    let outcome = store.load();
    assert_eq!(outcome.settings, settings);
    assert_eq!(outcome.warning, None);
}

#[test]
fn disclosure_acknowledgement_round_trips_through_the_file() {
    // 002-first-launch-and-sign-in DM-27: the `[disclosure]` section must
    // survive a real save/load through `settings.toml` on disk, not just
    // the in-memory `RawSettings` conversion (`settings/model.rs`'s own
    // unit tests already cover that half).
    let dir = TempDir::new();
    let store = store_in(&dir);
    let settings = AudioSettings {
        disclosure: Some(DisclosureAcknowledgement {
            acknowledged_version: 1,
            acknowledged_at: time::OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(3600),
        }),
        ..AudioSettings::default()
    };
    assert!(store.save(&settings).is_ok());

    let outcome = store.load();
    assert_eq!(outcome.settings, settings);
    assert_eq!(outcome.warning, None);

    let on_disk = fs::read_to_string(store.path()).unwrap_or_default();
    assert!(
        on_disk.contains("[disclosure]") && on_disk.contains("acknowledged_version = 1"),
        "settings.toml must persist the [disclosure] section, got:\n{on_disk}"
    );
}

#[test]
fn out_of_range_file_values_clamp() {
    let dir = TempDir::new();
    let store = store_in(&dir);
    let content = "schema_version = 1\n\n[audio]\nlimiter_ceiling_db = 3.0\nmaster_volume = 250\n\n[audio.safe_volume]\ncap = -5\n";
    let _ = fs::write(store.path(), content);

    let outcome = store.load();
    assert_eq!(
        outcome.warning, None,
        "out-of-range numbers clamp silently, no notification"
    );
    assert_eq!(outcome.settings.limiter_ceiling_db.db(), -0.1);
    assert_eq!(outcome.settings.master_volume.value(), 100);
    assert_eq!(outcome.settings.safe_volume.cap, VolumePercent::new(0));
}

#[test]
fn garbage_file_loads_defaults_with_exactly_one_warning() {
    let dir = TempDir::new();
    let store = store_in(&dir);
    let _ = fs::write(store.path(), "this is { not valid toml at all");

    let outcome = store.load();
    assert_eq!(outcome.settings, AudioSettings::default());
    assert_eq!(outcome.warning, Some(SettingsWarning::Unreadable));
}

#[test]
fn simulated_crash_mid_write_leaves_prior_file_intact() {
    let dir = TempDir::new();
    let store = store_in(&dir);
    let original = AudioSettings {
        master_volume: VolumePercent::new(42),
        ..AudioSettings::default()
    };
    assert!(store.save(&original).is_ok());

    // Simulate a crash between writing `.tmp` and renaming it: leave a
    // half-written tmp file behind without ever calling `rename`.
    let tmp_path = store.path().with_file_name("settings.toml.tmp");
    let _ = fs::write(&tmp_path, "garbage that must never become the real file");

    let outcome = store.load();
    assert_eq!(
        outcome.settings, original,
        "prior file must survive an interrupted write"
    );
    assert_eq!(outcome.warning, None);
}

#[test]
fn unknown_key_and_newer_schema_version_behave_as_contracted() {
    let dir = TempDir::new();

    // Unknown key: ignored, no warning, known fields still apply.
    let store = store_in(&dir);
    let content = "schema_version = 1\nfuture_top_level_key = 123\n\n[audio]\nbuffer_preset = \"performance\"\n";
    let _ = fs::write(store.path(), content);
    let outcome = store.load();
    assert_eq!(outcome.warning, None);
    assert_eq!(outcome.settings.buffer_preset, BufferPreset::Performance);

    // Newer schema version: defaults + warning, and the file is not rewritten by load().
    let newer_dir = TempDir::new();
    let newer_store = store_in(&newer_dir);
    let _ = fs::write(newer_store.path(), "schema_version = 99\n");
    let before = fs::read_to_string(newer_store.path()).unwrap_or_default();

    let newer_outcome = newer_store.load();
    assert_eq!(newer_outcome.settings, AudioSettings::default());
    assert_eq!(newer_outcome.warning, Some(SettingsWarning::NewerVersion));

    let after = fs::read_to_string(newer_store.path()).unwrap_or_default();
    assert_eq!(before, after, "load() must not rewrite a newer-schema file");
}

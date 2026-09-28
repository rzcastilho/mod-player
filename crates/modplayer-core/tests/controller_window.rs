// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! `PlaybackController` `[window]` shadow-state façade tests
//! (018-window-sizing-and-responsive-dock, contract W2, W4.6):
//! `window_settings()` reads the shadow state with no I/O,
//! `set_dock_width`/`set_window_inner_size` persist only when the clamped
//! result actually changes the shadow, and the persisted value survives a
//! new controller constructed over the same store — mirrors
//! `controller_actions.rs`'s own `custom_binding_survives_controller_restart`
//! (T076) restart pattern.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use modplayer_audio_io::FakeBackend;
use modplayer_audio_source_synthetic::SyntheticHost;
use modplayer_core::PlaybackController;
use modplayer_core::settings::{SettingsStore, WindowSettings};

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-controller-window-{}-{}",
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

fn controller_over(store: SettingsStore) -> PlaybackController<FakeBackend, SyntheticHost> {
    PlaybackController::new(FakeBackend::new(vec![]), SyntheticHost::new(44_100), store)
}

#[test]
fn window_settings_reads_the_seeded_default_shadow() {
    let dir = TempDir::new();
    let controller = controller_over(store_in(&dir));
    assert_eq!(controller.window_settings(), WindowSettings::default());
}

#[test]
fn set_dock_width_is_a_no_op_when_unchanged() {
    let dir = TempDir::new();
    let store_path = dir.path().join("settings.toml");
    let mut controller = controller_over(store_in(&dir));
    // `PlaybackController::new` itself may write once (e.g. to persist a
    // freshly generated `connect_device_id`), so the no-write assertion
    // below compares content, not existence, before/after.
    let before = fs::read_to_string(&store_path).unwrap_or_default();

    // The shadow already holds the default (280.0) — setting it to the
    // same clamped value must not touch disk at all.
    controller.set_dock_width(WindowSettings::default().dock_width);
    assert_eq!(
        fs::read_to_string(&store_path).unwrap_or_default(),
        before,
        "an unchanged set_dock_width must not write settings.toml"
    );

    // A real change does write.
    controller.set_dock_width(320.0);
    assert_eq!(controller.window_settings().dock_width, 320.0);
    assert!(store_path.exists());

    // Setting it again to the same (already-current) value is a no-op:
    // remove the file, call the setter, and confirm it stays absent.
    let _ = fs::remove_file(&store_path);
    controller.set_dock_width(320.0);
    assert!(
        !store_path.exists(),
        "re-setting the same dock width must not persist again"
    );
}

#[test]
fn set_dock_width_clamps_and_ignores_invalid_input() {
    let dir = TempDir::new();
    let mut controller = controller_over(store_in(&dir));

    controller.set_dock_width(10.0);
    assert_eq!(controller.window_settings().dock_width, 240.0);

    controller.set_dock_width(1_000.0);
    assert_eq!(controller.window_settings().dock_width, 480.0);

    controller.set_dock_width(300.0);
    for invalid in [f32::NAN, f32::INFINITY, -5.0, 0.0] {
        controller.set_dock_width(invalid);
        assert_eq!(
            controller.window_settings().dock_width,
            300.0,
            "non-finite/non-positive input must be ignored"
        );
    }
}

#[test]
fn set_window_inner_size_is_a_no_op_when_unchanged() {
    let dir = TempDir::new();
    let store_path = dir.path().join("settings.toml");
    let mut controller = controller_over(store_in(&dir));
    let before = fs::read_to_string(&store_path).unwrap_or_default();

    let default = WindowSettings::default();
    controller.set_window_inner_size(default.inner_width, default.inner_height);
    assert_eq!(
        fs::read_to_string(&store_path).unwrap_or_default(),
        before,
        "an unchanged set_window_inner_size must not write settings.toml"
    );

    controller.set_window_inner_size(1_500.0, 900.0);
    assert_eq!(controller.window_settings().inner_width, 1_500.0);
    assert_eq!(controller.window_settings().inner_height, 900.0);
    assert!(store_path.exists());

    let _ = fs::remove_file(&store_path);
    controller.set_window_inner_size(1_500.0, 900.0);
    assert!(
        !store_path.exists(),
        "re-setting the same inner size must not persist again"
    );
}

#[test]
fn set_window_inner_size_clamps_and_ignores_invalid_input() {
    let dir = TempDir::new();
    let mut controller = controller_over(store_in(&dir));

    controller.set_window_inner_size(100.0, 100.0);
    assert_eq!(controller.window_settings().inner_width, 960.0);
    assert_eq!(controller.window_settings().inner_height, 640.0);

    controller.set_window_inner_size(1_500.0, 900.0);
    for invalid in [f32::NAN, f32::INFINITY, -5.0, 0.0] {
        controller.set_window_inner_size(invalid, 900.0);
        controller.set_window_inner_size(1_500.0, invalid);
        assert_eq!(controller.window_settings().inner_width, 1_500.0);
        assert_eq!(controller.window_settings().inner_height, 900.0);
    }
}

/// W4.6: the persisted value survives a new controller constructed over
/// the same store — a whole restart, not just the in-process shadow.
#[test]
fn dock_width_and_inner_size_survive_controller_restart() {
    let dir = TempDir::new();
    let store_path = dir.path().join("settings.toml");
    let mut controller = controller_over(store_in(&dir));

    controller.set_dock_width(360.0);
    controller.set_window_inner_size(1_600.0, 1_000.0);
    drop(controller);

    let reconstructed = controller_over(SettingsStore::with_path(&store_path));
    assert_eq!(
        reconstructed.window_settings(),
        WindowSettings {
            inner_width: 1_600.0,
            inner_height: 1_000.0,
            dock_width: 360.0,
        }
    );
}

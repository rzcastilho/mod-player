// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! `[window]` property tests (018-window-sizing-and-responsive-dock,
//! contract W4.4/W4.5, data-model.md §1's round-trip invariant):
//!
//! - W4.4: any valid `WindowSettings` round-trips exactly through a real
//!   `SettingsStore::save`/`load`.
//! - W4.5: any arbitrary TOML float/string thrown at an individual
//!   `[window]` key loads to a value within that key's valid post-load
//!   range — never a crash, never a corrupted other section.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use modplayer_core::settings::{
    AudioSettings, DOCK_WIDTH_DEFAULT, DOCK_WIDTH_MAX, DOCK_WIDTH_MIN, MIN_INNER_SIZE, RawSettings,
    RawWindow, SettingsStore, WindowSettings,
};
use proptest::prelude::*;

/// A minimal self-cleaning temp directory (no `tempfile` dependency;
/// mirrors `settings.rs`'s own `TempDir`).
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-window-proptest-{}-{}",
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

/// Any already-valid `WindowSettings` (data-model.md §1's post-load
/// ranges): width/height at or above their minimums (no upper bound),
/// dock width in `[240, 480]`.
fn valid_window_settings() -> impl Strategy<Value = WindowSettings> {
    (
        MIN_INNER_SIZE.0..10_000.0f32,
        MIN_INNER_SIZE.1..10_000.0f32,
        DOCK_WIDTH_MIN..=DOCK_WIDTH_MAX,
    )
        .prop_map(|(inner_width, inner_height, dock_width)| WindowSettings {
            inner_width,
            inner_height,
            dock_width,
        })
}

proptest! {
    /// W4.4: any valid `WindowSettings` round-trips exactly through a real
    /// save/load — `load(save(x)) == x`.
    #[test]
    fn valid_window_settings_round_trip_through_save_load(window in valid_window_settings()) {
        let dir = TempDir::new();
        let store = store_in(&dir);
        let settings = AudioSettings {
            window,
            ..AudioSettings::default()
        };
        prop_assert!(store.save(&settings).is_ok());

        let outcome = store.load();
        prop_assert_eq!(outcome.settings.window, window);
        prop_assert!(outcome.warnings.is_empty());
    }
}

/// A raw `[window]` value that may or may not be well-formed: absent,
/// an arbitrary `f64` (including non-finite/negative/zero), or an
/// arbitrary string — every case `into_settings` must handle without
/// panicking or discarding the rest of the file (contract W1).
fn any_raw_window_value() -> impl Strategy<Value = Option<toml::Value>> {
    prop_oneof![
        Just(None),
        any::<f64>().prop_map(|f| Some(toml::Value::Float(f))),
        ".*".prop_map(|s: String| Some(toml::Value::String(s))),
    ]
}

proptest! {
    /// W4.5: any arbitrary `f64`/string per `[window]` key loads to a
    /// value within that key's valid post-load range — independent of
    /// what garbage the other two keys hold.
    #[test]
    fn any_raw_window_value_loads_within_valid_ranges(
        inner_width in any_raw_window_value(),
        inner_height in any_raw_window_value(),
        dock_width in any_raw_window_value(),
    ) {
        let raw = RawSettings {
            window: RawWindow {
                inner_width,
                inner_height,
                dock_width,
            },
            ..RawSettings::default()
        };
        let (settings, invalid, dropped) = raw.into_settings();

        prop_assert!(dropped.is_empty());
        // `[window]` never raises an `InvalidField` (silent per-key
        // fallback, contract W1) — any entries here would come from an
        // unrelated default field, none of which this test touches.
        prop_assert!(invalid.is_empty());

        prop_assert!(settings.window.inner_width.is_finite());
        prop_assert!(settings.window.inner_width >= MIN_INNER_SIZE.0);
        prop_assert!(settings.window.inner_height.is_finite());
        prop_assert!(settings.window.inner_height >= MIN_INNER_SIZE.1);
        prop_assert!(settings.window.dock_width.is_finite());
        prop_assert!(settings.window.dock_width >= DOCK_WIDTH_MIN);
        prop_assert!(settings.window.dock_width <= DOCK_WIDTH_MAX);
    }
}

/// A sanity check on the default, outside any generated case: the
/// documented defaults (data-model.md §1) are exactly what an absent
/// `[window]` table loads to.
#[test]
fn defaults_match_documented_constants() {
    let defaults = WindowSettings::default();
    assert_eq!(defaults.inner_width, 1200.0);
    assert_eq!(defaults.inner_height, 820.0);
    assert_eq!(defaults.dock_width, DOCK_WIDTH_DEFAULT);
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! The settings store: load/save `settings.toml` per the read and write
//! rules in contracts/settings-file.md. Writes are atomic (`.tmp` +
//! `sync_all()` + `rename`); loads never fail outright — every error path
//! degrades to defaults plus, where the contract calls for one, a single
//! warning.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;

use super::model::{AudioSettings, InvalidField, RawSettings, SCHEMA_VERSION};

/// Environment variable that overrides the settings directory (tests and
/// portable use; contracts/settings-file.md).
pub const CONFIG_DIR_ENV: &str = "MODPLAYER_CONFIG_DIR";

const FILE_NAME: &str = "settings.toml";
const TMP_FILE_NAME: &str = "settings.toml.tmp";

/// A warning raised while loading settings, mapped to a Fluent message key
/// by the caller (`modplayer-core::notifications`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsWarning {
    /// The file existed but could not be parsed as TOML.
    Unreadable,
    /// `schema_version` is newer than this build understands.
    NewerVersion,
    /// One or more enum fields held an unrecognised value.
    InvalidValue(Vec<InvalidField>),
    /// One or more `[keybindings]` entries were dropped in isolation
    /// (007, contracts/keymap-settings.md): an unknown action id, a
    /// value that isn't an array of strings, or an unparseable chord
    /// string. Lists the dropped action ids.
    InvalidKeybindings(Vec<String>),
}

impl SettingsWarning {
    /// The Fluent message key for this warning (contracts/settings-file.md;
    /// `InvalidKeybindings` — contracts/keymap-settings.md).
    pub fn message_key(&self) -> &'static str {
        match self {
            SettingsWarning::Unreadable => "settings-unreadable",
            SettingsWarning::NewerVersion => "settings-newer-version",
            SettingsWarning::InvalidValue(_) => "settings-invalid-value",
            SettingsWarning::InvalidKeybindings(_) => "keybindings-invalid-entries",
        }
    }
}

/// The result of a `SettingsStore::load()` call. `warnings` holds, in
/// order, `InvalidValue` (if any) then `InvalidKeybindings` (if any) —
/// or `Unreadable`/`NewerVersion` alone, since those short-circuit the
/// rest of the load (contracts/keymap-settings.md).
#[derive(Debug, Clone, PartialEq)]
pub struct LoadOutcome {
    pub settings: AudioSettings,
    pub warnings: Vec<SettingsWarning>,
}

/// Errors saving settings. Loading never returns an error: every failure
/// mode degrades to defaults plus (per contract) a warning.
#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    #[error("could not write settings: {0}")]
    Io(#[from] std::io::Error),
    #[error("could not serialize settings: {0}")]
    Serialize(#[from] toml::ser::Error),
}

/// Loads and saves `settings.toml` at a resolved platform (or overridden)
/// config path.
#[derive(Debug, Clone)]
pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    /// Resolve the store's path from `MODPLAYER_CONFIG_DIR` (if set) or the
    /// platform's default config directory (contracts/settings-file.md).
    /// Returns `None` only if neither the override nor the platform
    /// directories crate can determine a home directory.
    pub fn new() -> Option<Self> {
        Some(Self {
            path: config_dir()?.join(FILE_NAME),
        })
    }

    /// Point the store directly at `path` (used by tests, bypassing both
    /// the environment override and platform directory resolution).
    pub fn with_path(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The resolved `settings.toml` path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Load settings, degrading to defaults on any failure
    /// (contracts/settings-file.md's read-rules table).
    pub fn load(&self) -> LoadOutcome {
        if !self.path.exists() {
            return LoadOutcome {
                settings: AudioSettings::default(),
                warnings: Vec::new(),
            };
        }

        let content = match fs::read_to_string(&self.path) {
            Ok(content) => content,
            Err(_) => {
                return LoadOutcome {
                    settings: AudioSettings::default(),
                    warnings: vec![SettingsWarning::Unreadable],
                };
            }
        };

        let raw: RawSettings = match toml::from_str(&content) {
            Ok(raw) => raw,
            Err(_) => {
                return LoadOutcome {
                    settings: AudioSettings::default(),
                    warnings: vec![SettingsWarning::Unreadable],
                };
            }
        };

        if raw.schema_version > SCHEMA_VERSION {
            return LoadOutcome {
                settings: AudioSettings::default(),
                warnings: vec![SettingsWarning::NewerVersion],
            };
        }

        let (settings, invalid, invalid_keybindings) = raw.into_settings();
        let mut warnings = Vec::new();
        if !invalid.is_empty() {
            warnings.push(SettingsWarning::InvalidValue(invalid));
        }
        if !invalid_keybindings.is_empty() {
            warnings.push(SettingsWarning::InvalidKeybindings(invalid_keybindings));
        }
        LoadOutcome { settings, warnings }
    }

    /// Save `settings`, serializing the full struct and replacing the file
    /// atomically: write `settings.toml.tmp`, `sync_all()`, then `rename`
    /// over the target (contracts/settings-file.md's write rules).
    pub fn save(&self, settings: &AudioSettings) -> Result<(), SaveError> {
        let raw = RawSettings::from_settings(settings);
        let serialized = toml::to_string_pretty(&raw)?;

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let tmp_path = self.path.with_file_name(TMP_FILE_NAME);
        {
            let mut file = fs::File::create(&tmp_path)?;
            file.write_all(serialized.as_bytes())?;
            file.sync_all()?;
        }
        fs::rename(&tmp_path, &self.path)?;
        Ok(())
    }
}

fn config_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var(CONFIG_DIR_ENV) {
        return Some(PathBuf::from(dir));
    }
    ProjectDirs::from("", "ModPlayer", "ModPlayer").map(|dirs| dirs.config_dir().to_path_buf())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;
    use modplayer_engine::{BufferPreset, VolumePercent};
    use std::sync::atomic::{AtomicU64, Ordering};

    /// A minimal self-cleaning temp directory, avoiding a `tempfile`/
    /// `tempdir` dev-dependency for a handful of settings-store tests.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!(
                "modplayer-settings-test-{}-{}",
                std::process::id(),
                unique
            ));
            fs::create_dir_all(&dir).expect("create temp dir");
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

    fn temp_store() -> (SettingsStore, TempDir) {
        let dir = TempDir::new();
        let store = SettingsStore::with_path(dir.path().join(FILE_NAME));
        (store, dir)
    }

    #[test]
    fn missing_file_loads_defaults_with_no_warning() {
        let (store, _dir) = temp_store();
        let outcome = store.load();
        assert_eq!(outcome.settings, AudioSettings::default());
        assert!(outcome.warnings.is_empty());
    }

    #[test]
    fn round_trip_defaults() {
        let (store, _dir) = temp_store();
        let settings = AudioSettings::default();
        store.save(&settings).expect("save");
        let outcome = store.load();
        assert_eq!(outcome.settings, settings);
        assert!(outcome.warnings.is_empty());
    }

    #[test]
    fn out_of_range_file_values_clamp() {
        let (store, _dir) = temp_store();
        let content = "schema_version = 1\n\n[audio]\nlimiter_ceiling_db = 3.0\nmaster_volume = 250\n\n[audio.safe_volume]\ncap = -5\n";
        fs::write(store.path(), content).expect("write");
        let outcome = store.load();
        assert!(outcome.warnings.is_empty());
        assert_eq!(outcome.settings.limiter_ceiling_db.db(), -0.1);
        assert_eq!(outcome.settings.master_volume.value(), 100);
        assert_eq!(outcome.settings.safe_volume.cap.value(), 0);
    }

    #[test]
    fn garbage_file_loads_defaults_with_exactly_one_warning() {
        let (store, _dir) = temp_store();
        fs::write(store.path(), "this is not { valid toml").expect("write");
        let outcome = store.load();
        assert_eq!(outcome.settings, AudioSettings::default());
        assert_eq!(outcome.warnings, vec![SettingsWarning::Unreadable]);
    }

    #[test]
    fn simulated_crash_mid_write_leaves_prior_file_intact() {
        let (store, _dir) = temp_store();
        let original = AudioSettings {
            master_volume: VolumePercent::new(42),
            ..AudioSettings::default()
        };
        store.save(&original).expect("save");

        // Simulate a crash after writing the `.tmp` file but before the
        // rename: write garbage to the tmp path only, leave the real file
        // untouched.
        let tmp_path = store.path().with_file_name(TMP_FILE_NAME);
        fs::write(&tmp_path, "garbage, never renamed").expect("write tmp");

        let outcome = store.load();
        assert_eq!(outcome.settings, original);
        assert!(outcome.warnings.is_empty());
    }

    #[test]
    fn unknown_key_is_ignored() {
        let (store, _dir) = temp_store();
        let content = "schema_version = 1\nsome_future_top_level_key = true\n\n[audio]\nbuffer_preset = \"safe\"\nunknown_audio_key = 123\n";
        fs::write(store.path(), content).expect("write");
        let outcome = store.load();
        assert!(outcome.warnings.is_empty());
        assert_eq!(outcome.settings.buffer_preset, BufferPreset::Safe);
    }

    #[test]
    fn newer_schema_version_loads_defaults_and_is_not_rewritten() {
        let (store, _dir) = temp_store();
        fs::write(store.path(), "schema_version = 99\n").expect("write");
        let before = fs::read_to_string(store.path()).expect("read");

        let outcome = store.load();
        assert_eq!(outcome.settings, AudioSettings::default());
        assert_eq!(outcome.warnings, vec![SettingsWarning::NewerVersion]);

        let after = fs::read_to_string(store.path()).expect("read");
        assert_eq!(before, after, "load() must not rewrite the file");
    }
}

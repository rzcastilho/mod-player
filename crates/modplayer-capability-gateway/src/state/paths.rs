// SPDX-License-Identifier: MIT OR Apache-2.0

//! Resolves where a plugin's key-value state lives on disk (research R9,
//! R22; mirrors `markers::store::TrackStatePaths`).

use std::fs;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;

/// Overrides the plugin-state directory (tests; mirrors
/// `MODPLAYER_TRACK_STATE_DIR`).
pub const PLUGIN_STATE_DIR_ENV: &str = "MODPLAYER_PLUGIN_STATE_DIR";

/// Lowercase hex of `identifier`'s ASCII bytes — filesystem-safe on every
/// platform without escaping (mirrors `markers::store::encode_track_id`).
fn encode_identifier(identifier: &str) -> String {
    let mut out = String::with_capacity(identifier.len() * 2);
    for byte in identifier.as_bytes() {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Lowercase hex of a track id's ASCII bytes, for `tracks/<hex>.json`.
fn encode_track(track_id: &str) -> String {
    encode_identifier(track_id)
}

/// Resolved plugin-state root directory (data-model.md §1.7).
#[derive(Debug, Clone)]
pub struct PluginStatePaths {
    root: PathBuf,
}

impl PluginStatePaths {
    /// `MODPLAYER_PLUGIN_STATE_DIR` if set, else the platform data-local
    /// dir's `ModPlayer/plugin-state/`. `None` only if neither is
    /// determinable (in-memory-only state, like `AnalysisPaths`).
    #[must_use]
    pub fn resolve() -> Option<Self> {
        if let Ok(dir) = std::env::var(PLUGIN_STATE_DIR_ENV) {
            return Some(Self::with_dir(dir));
        }
        ProjectDirs::from("", "ModPlayer", "ModPlayer")
            .map(|dirs| Self::with_dir(dirs.data_local_dir().join("plugin-state")))
    }

    #[must_use]
    pub fn with_dir(dir: impl Into<PathBuf>) -> Self {
        Self { root: dir.into() }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `<root>/<hex(identifier)>/plugin.json`.
    #[must_use]
    pub fn plugin_file(&self, identifier: &str) -> PathBuf {
        self.root
            .join(encode_identifier(identifier))
            .join("plugin.json")
    }

    /// `<root>/<hex(identifier)>/tracks/<hex(track)>.json`.
    #[must_use]
    pub fn track_file(&self, identifier: &str, track_id: &str) -> PathBuf {
        self.root
            .join(encode_identifier(identifier))
            .join("tracks")
            .join(format!("{}.json", encode_track(track_id)))
    }

    /// `<root>/<hex(identifier)>/tracks/`.
    #[must_use]
    pub fn tracks_dir(&self, identifier: &str) -> PathBuf {
        self.root.join(encode_identifier(identifier)).join("tracks")
    }

    /// Sign-out (FR-014): delete every plugin's per-track scope, keeping
    /// `plugin.json` untouched. Best-effort — a missing directory is not
    /// an error.
    pub fn clear_tracks(&self, identifier: &str) {
        let _ = fs::remove_dir_all(self.tracks_dir(identifier));
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;

    #[test]
    fn identifier_encodes_to_lowercase_hex() {
        let paths = PluginStatePaths::with_dir("/tmp/plugin-state");
        let file = paths.plugin_file("org.modplayer.fixture.wellbehaved");
        let hex = file
            .parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap();
        assert!(
            hex.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
    }

    #[test]
    fn track_file_is_under_tracks_dir() {
        let paths = PluginStatePaths::with_dir("/tmp/plugin-state");
        let track = paths.track_file("org.modplayer.fixture.wellbehaved", "spotify:track:x");
        assert!(track.starts_with(paths.tracks_dir("org.modplayer.fixture.wellbehaved")));
    }
}

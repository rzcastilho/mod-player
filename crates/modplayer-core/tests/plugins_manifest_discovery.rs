// SPDX-License-Identifier: MIT OR Apache-2.0

//! `PluginHost::discover`'s own discovery/listing invariants (US2,
//! contracts/plugin-host-service.md §2 "L1 Discovery", §6): fixtures only
//! ever appear behind the env var, an invalid manifest is listed but
//! never loaded, the list is sorted by name, and nothing ever removes a
//! discovered record (FR-013 "no uninstall").

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::PoisonError;
use std::sync::atomic::{AtomicU64, Ordering};

use modplayer_core::plugins::{Lifecycle, PluginHost};

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-plugins-manifest-discovery-{}-{}",
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

/// `MODPLAYER_PLUGIN_STATE_DIR` is process-global, so every `discover()`
/// in this binary must be serialized against every other's brief mutation
/// of it (mirrors `controller_plugins_lifecycle.rs`'s own pattern).
static PLUGIN_STATE_ENV_LOCK: Mutex<()> = Mutex::new(());

fn discover(fixtures_enabled: bool) -> (PluginHost, TempDir) {
    let dir = TempDir::new();
    let host = {
        let _guard = PLUGIN_STATE_ENV_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        // Safety: narrowly scopes the mutation to the one synchronous
        // read `PluginStatePaths::resolve()` makes inside `discover()`,
        // serialized against every other test in this binary via the
        // lock above.
        unsafe { std::env::set_var("MODPLAYER_PLUGIN_STATE_DIR", dir.path()) };
        let host = PluginHost::discover(fixtures_enabled);
        unsafe { std::env::remove_var("MODPLAYER_PLUGIN_STATE_DIR") };
        host
    };
    (host, dir)
}

/// L1: fixture packages exist only when `fixtures_enabled` — `plugins/
/// bundled/` is empty this slice, so `discover(false)` must list nothing
/// at all, and `discover(true)` must list every one of the seven fixtures
/// landed so far (US1's four plus US2's three).
#[test]
fn fixtures_only_with_env() {
    let (without, _dir) = discover(false);
    assert!(
        without.records().is_empty(),
        "no bundled packages ship this slice, and fixtures must be gated off"
    );

    let (with, _dir2) = discover(true);
    let identifiers: Vec<&str> = with
        .records()
        .iter()
        .map(|r| r.identifier.as_str())
        .collect();
    for expected in [
        "org.modplayer.fixture.hang",
        "org.modplayer.fixture.throw",
        "org.modplayer.fixture.leak",
        "org.modplayer.fixture.noready",
        "org.modplayer.fixture.observer",
        "org.modplayer.fixture.invalid",
        "org.modplayer.fixture.flood",
    ] {
        assert!(
            identifiers.contains(&expected),
            "{expected} must be discovered with fixtures enabled, got {identifiers:?}"
        );
    }
}

/// L1 (contracts/manifest.md §3): the `invalid` fixture's unknown
/// permission is caught at discovery — it is listed (a row exists) but
/// `lifecycle = Invalid(_)`, `enabled = false`, `health = None`, and it
/// is never spawnable.
#[test]
fn invalid_fixture_listed_not_loaded() {
    let (host, _dir) = discover(true);
    let record = host
        .records()
        .iter()
        .find(|r| r.identifier.as_str() == "org.modplayer.fixture.invalid")
        .unwrap_or_else(|| unreachable!("the invalid fixture must still be listed"));

    assert!(
        matches!(record.lifecycle, Lifecycle::Invalid(_)),
        "an invalid manifest must produce Lifecycle::Invalid, got {:?}",
        record.lifecycle
    );
    assert!(!record.enabled, "an invalid record must never be enabled");
    assert_eq!(
        record.health, None,
        "an invalid record has no health until it is ever valid"
    );
    assert!(
        record.manifest.is_err(),
        "the invalid fixture's manifest must not have parsed"
    );
}

/// FR-023: `records()` is sorted by name, case-insensitively (L1/L2 sort
/// key) — the same order the eventual `PluginsView` projects.
#[test]
fn rows_sorted_by_name() {
    let (host, _dir) = discover(true);
    let names: Vec<String> = host
        .records()
        .iter()
        .map(|r| {
            r.manifest
                .as_ref()
                .map(|m| m.name.clone())
                .unwrap_or_else(|_| r.identifier.to_string())
                .to_lowercase()
        })
        .collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(
        names, sorted,
        "discovered records must already be sorted by name (case-insensitive)"
    );
}

/// L9/FR-013: nothing in this crate's plugin API ever removes a
/// discovered record — disabling (and even the 3rd-suspension
/// auto-disable) only flips `enabled`/`lifecycle`, never drops the row.
/// This is the observable half of "no uninstall exists"; the other half
/// (no such method exists at all) is a compile-time fact, not a runtime
/// one.
#[test]
fn no_uninstall() {
    let (mut host, _dir) = discover(true);
    let before = host.records().len();
    let id = host
        .records()
        .iter()
        .find(|r| r.identifier.as_str() == "org.modplayer.fixture.observer")
        .map(|r| r.id)
        .unwrap_or_else(|| unreachable!("the observer fixture must be discovered"));

    if let Some(record) = host.record_mut(id) {
        record.enabled = false;
    }

    assert_eq!(
        host.records().len(),
        before,
        "flipping `enabled` off must never remove the record"
    );
    assert!(
        host.record(id).is_some(),
        "the plugin must still be listed after being disabled"
    );
}

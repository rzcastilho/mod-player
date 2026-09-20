// SPDX-License-Identifier: MIT OR Apache-2.0

//! `PluginHost::discover`'s own discovery/listing invariants (US2,
//! contracts/plugin-host-service.md §2 "L1 Discovery", §6): fixtures only
//! ever appear behind the env var, an invalid manifest is listed but
//! never loaded, the list is sorted by name, and nothing ever removes a
//! discovered record (FR-013 "no uninstall"). 013-key-and-tempo-plugin
//! (research R8) adds a second always-on bundled package, Key & Tempo,
//! alongside Section Loop.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::PoisonError;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use modplayer_audio_io::{FakeBackend, FakeDevice};
use modplayer_audio_source_synthetic::ScriptedHost;
use modplayer_core::PlaybackController;
use modplayer_core::plugins::{Lifecycle, PluginHost, PluginId};
use modplayer_core::settings::SettingsStore;
use modplayer_engine::{BufferPreset, DeviceId, FrameCount, SampleRate};

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

/// L1: fixture packages exist only when `fixtures_enabled` — 012-
/// section-loop-plugin (research R5): `plugins/bundled/` ships Section
/// Loop, always present regardless of the env var; 013-key-and-tempo-
/// plugin (research R8) adds Key & Tempo the same way, so
/// `discover(false)` must list exactly both, sorted by display name
/// (`record_sort_key`, case-insensitive: "Key & Tempo" before "Section
/// Loop"), and `discover(true)` must additionally list every one of the
/// fixtures landed so far.
#[test]
fn fixtures_only_with_env() {
    let (without, _dir) = discover(false);
    let without_identifiers: Vec<&str> = without
        .records()
        .iter()
        .map(|r| r.identifier.as_str())
        .collect();
    assert_eq!(
        without_identifiers,
        vec!["org.modplayer.key-tempo", "org.modplayer.section-loop"],
        "without fixtures, exactly the two bundled packages must be discovered, sorted by name"
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

/// 012-section-loop-plugin (research R5): the 009 FR-023 zero-row empty
/// state is still implemented even though the real `bundled::packages()`
/// can no longer produce it — exercised here through
/// `PluginHost::discover_packages(Vec::new())`, an explicit empty package
/// list, rather than through `discover()`.
#[test]
fn discover_packages_empty_list_is_empty_state() {
    let dir = TempDir::new();
    let host = {
        let _guard = PLUGIN_STATE_ENV_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        // Safety: mirrors `discover()`'s own narrowly-scoped mutation.
        unsafe { std::env::set_var("MODPLAYER_PLUGIN_STATE_DIR", dir.path()) };
        let host = PluginHost::discover_packages(Vec::new());
        unsafe { std::env::remove_var("MODPLAYER_PLUGIN_STATE_DIR") };
        host
    };
    assert!(
        host.records().is_empty(),
        "an explicit empty package list must produce zero records"
    );
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

// -----------------------------------------------------------------------
// 013-key-and-tempo-plugin (Phase 2 Foundational, research R8,
// Constitution IX §7): a second always-on bundled package alongside
// Section Loop, and proof the API 1.4 schema bump broke no existing
// 1.0-1.3 manifest.
// -----------------------------------------------------------------------

fn fake_device(id: &str, is_default: bool) -> FakeDevice {
    FakeDevice {
        id: DeviceId::new(id).unwrap_or_else(|| unreachable!()),
        name: id.to_string(),
        rate: SampleRate::new(44_100),
        channels: 2,
        buffer_range: Some((FrameCount::new(32), FrameCount::new(2_048))),
        is_default,
    }
}

fn pump_until(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    timeout: Duration,
    mut done: impl FnMut(&mut PlaybackController<FakeBackend, ScriptedHost>) -> bool,
) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        controller.tick();
        if done(controller) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn wait_active(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    id: PluginId,
) -> bool {
    pump_until(controller, Duration::from_secs(5), |c| {
        matches!(
            c.plugins_mut().record(id).map(|r| &r.lifecycle),
            Some(Lifecycle::Active)
        )
    })
}

/// P4/G1 (contracts/key-tempo-plugin.md §1, data-model.md §2.7): from a
/// fresh launch, with no fixtures involved, both Section Loop and Key &
/// Tempo reach `Active`, every permission their own manifest declares is
/// granted, and neither can be uninstalled (FR-013).
#[test]
fn both_bundled_plugins_enabled_with_grants() {
    let dir = TempDir::new();
    let plugin_state_dir = TempDir::new();
    let track_state_dir = TempDir::new();
    let store = SettingsStore::with_path(dir.path().join("settings.toml"));
    let host = ScriptedHost::new();
    let devices = vec![fake_device("dev-1", true)];
    let mut controller = {
        let _guard = PLUGIN_STATE_ENV_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        // Safety: see `discover()`'s own narrowly-scoped mutation note.
        unsafe {
            std::env::set_var("MODPLAYER_PLUGIN_STATE_DIR", plugin_state_dir.path());
            std::env::set_var("MODPLAYER_TRACK_STATE_DIR", track_state_dir.path());
        }
        let controller = PlaybackController::new(FakeBackend::new(devices), host, store);
        unsafe {
            std::env::remove_var("MODPLAYER_PLUGIN_STATE_DIR");
            std::env::remove_var("MODPLAYER_TRACK_STATE_DIR");
        }
        controller
    };
    controller.launch();
    let dev_id = DeviceId::new("dev-1").unwrap_or_else(|| unreachable!());
    controller.confirm_device(dev_id, BufferPreset::Balanced);

    for identifier in ["org.modplayer.section-loop", "org.modplayer.key-tempo"] {
        let id = controller
            .plugins_mut()
            .records()
            .iter()
            .find(|r| r.identifier.as_str() == identifier)
            .map(|r| r.id)
            .unwrap_or_else(|| unreachable!("{identifier} must be discovered"));
        assert!(wait_active(&mut controller, id), "{identifier} must launch");
    }

    let mut required_by_identifier = Vec::new();
    for identifier in ["org.modplayer.section-loop", "org.modplayer.key-tempo"] {
        let required: Vec<modplayer_capability_gateway::api::Permission> = controller
            .plugins_mut()
            .records()
            .iter()
            .find(|r| r.identifier.as_str() == identifier)
            .unwrap_or_else(|| unreachable!("{identifier} must be discovered"))
            .manifest
            .as_ref()
            .unwrap_or_else(|e| unreachable!("{identifier} manifest must validate: {e}"))
            .required
            .iter()
            .map(|p| p.permission)
            .collect();
        required_by_identifier.push((identifier, required));
    }

    let view = controller.plugins_view();
    for (identifier, required) in required_by_identifier {
        let row = view
            .rows
            .iter()
            .find(|r| r.identifier.as_str() == identifier)
            .unwrap_or_else(|| unreachable!("{identifier} must have a Plugins-list row"));

        assert!(row.enabled, "{identifier} must be enabled by default");
        assert!(
            !row.can_uninstall,
            "{identifier} is bundled: it can never be uninstalled (FR-013)"
        );
        for permission in required {
            assert!(
                row.permissions.contains(&permission),
                "{identifier}: {} must be granted",
                permission.name()
            );
        }
    }
}

/// Constitution IX §7 (contracts/plugin-api-v1.4.md, additive-only):
/// bumping the schema to 1.4 breaks no existing 1.0-1.3 manifest —
/// proven end to end through `PluginHost::discover` (parse, validate and
/// assign a lifecycle) rather than just the gateway crate's own
/// `parse_and_validate`. Section Loop's own `api = "1.3"` is untouched.
#[test]
fn legacy_1_3_fixtures_still_load() {
    let (host, _dir) = discover(true);
    for record in host.records() {
        if record.identifier.as_str() == "org.modplayer.fixture.invalid" {
            continue; // deliberately invalid (unknown permission); unrelated to the api bump
        }
        assert!(
            record.manifest.is_ok(),
            "{}: the api 1.4 bump must not break discovery of an existing manifest: {:?}",
            record.identifier,
            record.manifest.as_ref().err()
        );
    }

    let section_loop = host
        .records()
        .iter()
        .find(|r| r.identifier.as_str() == "org.modplayer.section-loop")
        .unwrap_or_else(|| unreachable!("Section Loop must be discovered"));
    let m = section_loop
        .manifest
        .as_ref()
        .unwrap_or_else(|e| unreachable!("Section Loop manifest must still validate: {e}"));
    assert_eq!(m.api.major, 1);
    assert_eq!(
        m.api.min_minor, 3,
        "Section Loop's own api = \"1.3\" is untouched by the 1.4 bump"
    );
}

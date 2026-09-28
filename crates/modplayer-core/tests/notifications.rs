// SPDX-License-Identifier: MIT OR Apache-2.0

//! Notification lifecycle contract (FR-016, data-model.md §6.4): `Info`
//! auto-dismisses at 10 s; `Warning`/`Critical` persist until manually
//! dismissed. 019-notification-presentation Phase 6/US4 (T019) extends
//! this file with the raise-site changes contract C4 describes: every
//! device/keybindings notification a `PlaybackController` raises now
//! carries the args/detail split C4 specifies, with severity/trigger/
//! action set unchanged (FR-017).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use modplayer_audio_io::FakeBackend;
use modplayer_audio_source_synthetic::SyntheticHost;
use modplayer_core::settings::SettingsStore;
use modplayer_core::{
    NotificationAction, NotificationCenter, PlaybackController, PluginId, Severity,
};
use modplayer_engine::{BufferPreset, DeviceId, FrameCount, SampleRate};

#[test]
fn info_auto_dismisses_at_ten_seconds() {
    let mut center = NotificationCenter::new();
    let id = center.raise(Severity::Info, "device-available-again");
    let raised_at = Instant::now();

    center.tick(raised_at + Duration::from_millis(9_999));
    assert!(
        center.visible().any(|n| n.id == id),
        "must still be visible just under 10s"
    );

    center.tick(raised_at + Duration::from_secs(10));
    assert!(
        !center.visible().any(|n| n.id == id),
        "must be auto-dismissed at 10s"
    );
}

#[test]
fn warning_and_critical_persist_until_dismiss_is_called() {
    let mut center = NotificationCenter::new();
    let warning_id = center.raise(Severity::Warning, "settings-unreadable");
    let critical_id = center.raise(Severity::Critical, "no-output-devices");

    // Ticking far past the Info auto-dismiss window must not touch them.
    center.tick(Instant::now() + Duration::from_secs(24 * 60 * 60));
    assert!(center.visible().any(|n| n.id == warning_id));
    assert!(center.visible().any(|n| n.id == critical_id));

    center.dismiss(warning_id);
    assert!(!center.visible().any(|n| n.id == warning_id));
    assert!(
        center.visible().any(|n| n.id == critical_id),
        "dismissing one must not affect the other"
    );

    center.dismiss(critical_id);
    assert!(!center.visible().any(|n| n.id == critical_id));
}

/// 009 L6: `raise_keyed` dismisses any existing *visible* notification
/// with the same `dedupe_key` first, so a repeat suspension for the same
/// plugin replaces rather than stacks — mirrors `plugin-suspended:
/// <identifier>`'s own dedupe key.
#[test]
fn raise_keyed_replaces() {
    let mut center = NotificationCenter::new();
    let key = "plugin-suspended:org.modplayer.fixture.hang";

    let first = center.raise_keyed(
        Severity::Warning,
        "plugin-suspended",
        vec![
            ("plugin", "Hang fixture".to_string()),
            ("cause", "hung".to_string()),
        ],
        vec![
            NotificationAction::RestartPlugin(PluginId(0)),
            NotificationAction::DisablePlugin(PluginId(0)),
        ],
        key,
    );
    assert!(center.visible().any(|n| n.id == first));

    let second = center.raise_keyed(
        Severity::Warning,
        "plugin-suspended",
        vec![
            ("plugin", "Hang fixture".to_string()),
            ("cause", "hung".to_string()),
        ],
        vec![
            NotificationAction::RestartPlugin(PluginId(0)),
            NotificationAction::DisablePlugin(PluginId(0)),
        ],
        key,
    );

    assert!(
        !center.visible().any(|n| n.id == first),
        "the earlier notification sharing this dedupe key must be dismissed"
    );
    assert!(center.visible().any(|n| n.id == second));
    assert_eq!(
        center
            .visible()
            .filter(|n| n.dedupe_key.as_deref() == Some(key))
            .count(),
        1,
        "a repeat raise_keyed must replace, never stack"
    );
}

// ---------------------------------------------------------------------
// 019-notification-presentation, Phase 6/US4 (T019, contract C4): raise-
// site changes through the real `PlaybackController` — mirrors
// `tests/device_policy.rs`'s own fixture helpers.
// ---------------------------------------------------------------------

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-core-notifications-integration-{}-{}",
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

fn fresh_store() -> (SettingsStore, TempDir) {
    let dir = TempDir::new();
    let store = SettingsStore::with_path(dir.path().join("settings.toml"));
    (store, dir)
}

fn fake_device(id: &str, name: &str, is_default: bool) -> modplayer_audio_io::FakeDevice {
    modplayer_audio_io::FakeDevice {
        id: DeviceId::new(id).unwrap_or_else(|| unreachable!("test id is non-empty")),
        name: name.to_string(),
        rate: SampleRate::new(44_100),
        channels: 2,
        buffer_range: Some((FrameCount::new(32), FrameCount::new(2048))),
        is_default,
    }
}

/// C4 `handle_device_lost` row: `raise_with_detail(Critical, device-lost,
/// [device, fallback], lost_id)` — the lost device's raw id lands only in
/// `detail`, never the message args; severity/actions unchanged.
#[test]
fn handle_device_lost_raises_with_detail_and_fallback_arg() {
    let (store, _dir) = fresh_store();
    let lost_id = DeviceId::new("dev-1").unwrap_or_else(|| unreachable!());
    let devices = vec![
        fake_device("dev-1", "USB Interface", true),
        fake_device("dev-2", "Built-in Speakers", false),
    ];
    let mut controller =
        PlaybackController::new(FakeBackend::new(devices), SyntheticHost::new(44_100), store);
    controller.launch();
    controller.confirm_device(lost_id.clone(), BufferPreset::Balanced);

    controller.backend_mut().remove_device(&lost_id);
    controller.tick();

    let notification = controller
        .notifications()
        .visible()
        .find(|n| n.message_key == "device-lost")
        .unwrap_or_else(|| unreachable!("device-lost must be raised"));
    assert_eq!(notification.severity, Severity::Critical);
    assert!(notification.actions.is_empty());
    assert_eq!(
        notification.args,
        vec![
            ("device", "USB Interface".to_string()),
            ("fallback", "Built-in Speakers".to_string()),
        ]
    );
    assert_eq!(
        notification.detail.as_deref(),
        Some(lost_id.to_string().as_str())
    );
}

/// C4 `raise_device_warning(MissingPreferred)` row, named-id case: the
/// resolved display name (not the raw id) is the `device` arg, the
/// fallback names the default device, and `detail` carries the raw id.
#[test]
fn missing_preferred_with_saved_name_raises_with_detail() {
    let missing_id = DeviceId::new("missing-dev").unwrap_or_else(|| unreachable!());
    let (store, _dir) = fresh_store();
    {
        let mut controller = PlaybackController::new(
            FakeBackend::new(vec![fake_device("missing-dev", "Scarlett 2i2", true)]),
            SyntheticHost::new(44_100),
            store.clone(),
        );
        controller.launch();
        controller.confirm_device(missing_id.clone(), BufferPreset::Balanced);
    }

    let devices = vec![fake_device("dev-2", "Built-in Speakers", true)];
    let mut controller =
        PlaybackController::new(FakeBackend::new(devices), SyntheticHost::new(44_100), store);
    controller.launch();

    let notification = controller
        .notifications()
        .visible()
        .find(|n| n.message_key == "device-missing-at-launch")
        .unwrap_or_else(|| unreachable!("device-missing-at-launch must be raised"));
    assert_eq!(notification.severity, Severity::Warning);
    assert_eq!(
        notification.args,
        vec![
            ("device", "Scarlett 2i2".to_string()),
            ("fallback", "Built-in Speakers".to_string()),
        ]
    );
    assert_eq!(
        notification.detail.as_deref(),
        Some(missing_id.to_string().as_str())
    );
}

/// C4 `handle_device_list_changed` (reappearance) row: `raise_with_detail`
/// with `[device]` and `detail = found.id`.
#[test]
fn device_available_again_raises_with_detail() {
    let (store, _dir) = fresh_store();
    let lost_id = DeviceId::new("dev-1").unwrap_or_else(|| unreachable!());
    let devices = vec![
        fake_device("dev-1", "USB Interface", true),
        fake_device("dev-2", "Built-in Speakers", false),
    ];
    let mut controller =
        PlaybackController::new(FakeBackend::new(devices), SyntheticHost::new(44_100), store);
    controller.launch();
    controller.confirm_device(lost_id.clone(), BufferPreset::Balanced);
    controller.backend_mut().remove_device(&lost_id);
    controller.tick();

    controller
        .backend_mut()
        .add_device(fake_device("dev-1", "USB Interface", false));
    controller.tick();

    let notification = controller
        .notifications()
        .visible()
        .find(|n| n.message_key == "device-available-again")
        .unwrap_or_else(|| unreachable!("device-available-again must be raised"));
    assert_eq!(notification.severity, Severity::Info);
    assert_eq!(
        notification.args,
        vec![("device", "USB Interface".to_string())]
    );
    assert_eq!(
        notification.detail.as_deref(),
        Some(lost_id.to_string().as_str())
    );
}

/// C4 `raise_settings_warning(InvalidKeybindings)` row: no args, dropped
/// ids joined into `detail` instead (research R11).
#[test]
fn keybindings_invalid_entries_raises_with_no_args_and_joined_detail() {
    let (store, _dir) = fresh_store();
    let content = "schema_version = 1\n\n[keybindings]\n\"host.nope.x\" = [\"A\"]\n\"host.nope.y\" = [\"B\"]\n";
    let _ = std::fs::write(store.path(), content);

    let controller =
        PlaybackController::new(FakeBackend::new(vec![]), SyntheticHost::new(44_100), store);

    let notification = controller
        .notifications()
        .all()
        .find(|n| n.message_key == "keybindings-invalid-entries")
        .unwrap_or_else(|| unreachable!("keybindings-invalid-entries must be raised"));
    assert_eq!(notification.severity, Severity::Warning);
    assert!(notification.args.is_empty());
    assert_eq!(
        notification.detail.as_deref(),
        Some("host.nope.x, host.nope.y")
    );
}

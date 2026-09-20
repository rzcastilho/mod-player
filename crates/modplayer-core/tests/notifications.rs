// SPDX-License-Identifier: MIT OR Apache-2.0

//! Notification lifecycle contract (FR-016, data-model.md §6.4): `Info`
//! auto-dismisses at 10 s; `Warning`/`Critical` persist until manually
//! dismissed.

use std::time::{Duration, Instant};

use modplayer_core::{NotificationAction, NotificationCenter, PluginId, Severity};

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

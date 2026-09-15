// SPDX-License-Identifier: MIT OR Apache-2.0

//! Every Fluent key the app shell, nav rail, notification area, and
//! Settings screens use (contracts/ui-surface.md "Main window",
//! "Notification message keys", "Settings") must resolve against
//! `locales/en-US/*.ftl` (research R10). Extended to cover every
//! Settings-screen key in US5 (T086).

use modplayer_core::{tr, tr_args};

/// Shell/nav/notification keys with no Fluent placeholder — resolved via
/// plain `tr`.
const SHELL_AND_NOTIFICATION_KEYS: &[&str] = &[
    // Nav rail.
    "nav-library",
    "nav-now-playing",
    "nav-plugins",
    "nav-settings",
    // Placeholder sections.
    "placeholder-library",
    "placeholder-plugins",
    // Notification area chrome.
    "severity-critical",
    "severity-warning",
    "severity-info",
    "notification-dismiss",
    // Notification message keys with no placeholder.
    "no-output-devices",
    "device-appeared",
    "settings-unreadable",
    "settings-newer-version",
    "settings-invalid-value",
    "settings-save-failed",
    "sample-notification-critical",
    "sample-notification-warning",
    "sample-notification-info",
    // Settings › Developer (US4, T083).
    "setting-buffer-frames",
    "setting-buffer-frames-desc",
    "setting-raise-notification",
    "setting-raise-notification-desc",
];

/// Every Settings-screen key (US5, T086): the eleven fixed-order category
/// labels, the search box, the placeholder text for categories with no
/// working settings yet, and every working setting's title/description
/// (contracts/ui-surface.md "Settings").
const SETTINGS_SCREEN_KEYS: &[&str] = &[
    // Category list, in fixed order.
    "settings-cat-account",
    "settings-cat-audio",
    "settings-cat-playback",
    "settings-cat-controls",
    "settings-cat-plugins",
    "settings-cat-offline",
    "settings-cat-appearance",
    "settings-cat-language",
    "settings-cat-developer",
    "settings-cat-privacy-diagnostics",
    "settings-cat-about",
    // Search box and placeholder-category content.
    "settings-search",
    "placeholder-settings-category",
    // Audio category.
    "setting-output-device",
    "setting-output-device-desc",
    "setting-buffer-preset",
    "setting-buffer-preset-desc",
    "setting-limiter-ceiling",
    "setting-limiter-ceiling-desc",
    "setting-safe-volume",
    "setting-safe-volume-desc",
    "setting-safe-volume-cap",
    "setting-safe-volume-cap-desc",
    "setting-test-output-device",
    "setting-test-output-device-desc",
    // Appearance category.
    "setting-theme",
    "setting-theme-desc",
    "setting-theme-system",
    "setting-theme-light",
    "setting-theme-dark",
    // Language category.
    "setting-locale",
    "setting-locale-desc",
    "language-english",
];

/// Notification message keys that take a `{ $device }` placeholder
/// (US3) — resolved via `tr_args` with a stand-in value, since a message
/// with an unresolved variable is a Fluent formatting error (and so
/// `tr()`/`try_lookup` reports it as "not found", not as a partial string).
const DEVICE_NAMED_KEYS: &[&str] = &[
    "device-lost",
    "device-missing-at-launch",
    "device-available-again",
];

#[test]
fn every_shell_nav_and_notification_key_resolves() {
    for key in SHELL_AND_NOTIFICATION_KEYS {
        let resolved = tr(key);
        assert_ne!(
            &resolved, key,
            "Fluent key `{key}` is missing from locales/en-US/*.ftl (tr() fell back to the raw key)"
        );
    }

    for key in SETTINGS_SCREEN_KEYS {
        let resolved = tr(key);
        assert_ne!(
            &resolved, key,
            "Fluent key `{key}` is missing from locales/en-US/*.ftl (tr() fell back to the raw key)"
        );
    }

    for key in DEVICE_NAMED_KEYS {
        let resolved = tr_args(key, &[("device", "Example Device".to_string())]);
        assert_ne!(
            &resolved, key,
            "Fluent key `{key}` is missing from locales/en-US/*.ftl (tr_args() fell back to the raw key)"
        );
    }
}

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
    "notification-action-sign-in",
    // 002-first-launch-and-sign-in launch-gate placeholders (Phase 2;
    // replaced by real Welcome/Sign-in screen keys in US1/US2).
    "launch-gate-placeholder-welcome",
    "launch-gate-placeholder-sign-in",
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

/// Every Welcome/Decline/Privacy-Notice key (US1, T037; contracts/
/// ui-surface.md "Welcome", "Privacy Notice") — hash-pinned content in
/// `locales/en-US/disclosure.ftl`.
const DISCLOSURE_KEYS: &[&str] = &[
    "welcome-title",
    "welcome-description",
    "disclosure-unofficial",
    "disclosure-premium-required",
    "disclosure-terms-apply",
    "disclosure-terms-link",
    "disclosure-privacy-link",
    "welcome-acknowledge",
    "welcome-decline",
    "decline-explanation",
    "decline-quit",
    "privacy-title",
    "privacy-body",
    "privacy-back",
];

/// Every sign-in/tier/account/about/notification key with no Fluent
/// placeholder (US2, T065-T072; contracts/ui-surface.md "Sign-in step",
/// "Settings › Account", "Settings › About") — resolved via plain `tr`.
const ACCOUNT_KEYS: &[&str] = &[
    "signin-title",
    "signin-explanation",
    "signin-note-cancelled",
    "signin-note-timed-out",
    "signin-note-service-error",
    "signin-note-previous-unfinished",
    "signin-note-revoked",
    "signin-start",
    "signin-retry",
    "signin-store-remedy-keychain",
    "signin-store-remedy-credential-manager",
    "signin-store-remedy-secret-service",
    "store-name-keychain",
    "store-name-credential-manager",
    "store-name-secret-service",
    "signin-waiting",
    "signin-waiting-resumed",
    "signin-open-again",
    "signin-cancel",
    "signin-checking",
    "tier-free-title",
    "tier-free-explanation",
    "tier-open-upgrade",
    "tier-continue",
    "tier-unknown-title",
    "tier-unknown-explanation",
    "tier-retry",
    "tier-premium",
    "tier-free",
    "tier-unknown",
    "account-tier",
    "account-never-validated",
    "account-signed-out",
    "account-sign-in",
    "account-recheck",
    "account-recheck-desc",
    "account-sign-out",
    "account-sign-out-desc",
    "about-product",
    "about-privacy",
    "about-disclosure",
    "signin-again",
];

/// Account/notification keys that take a Fluent placeholder — resolved via
/// `tr_args` with a stand-in value.
const ACCOUNT_ARG_KEYS: &[&str] = &[
    "signin-store-unavailable",
    "account-display-name",
    "account-last-validated",
    "about-version",
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

    for key in DISCLOSURE_KEYS {
        let resolved = tr(key);
        assert_ne!(
            &resolved, key,
            "Fluent key `{key}` is missing from locales/en-US/disclosure.ftl (tr() fell back to the raw key)"
        );
    }

    for key in ACCOUNT_KEYS {
        let resolved = tr(key);
        assert_ne!(
            &resolved, key,
            "Fluent key `{key}` is missing from locales/en-US/account.ftl (tr() fell back to the raw key)"
        );
    }

    for key in ACCOUNT_ARG_KEYS {
        let resolved = tr_args(
            key,
            &[
                ("store", "Keychain".to_string()),
                ("name", "Alex".to_string()),
                ("when", "just now".to_string()),
                ("version", "0.1.0".to_string()),
            ],
        );
        assert_ne!(
            &resolved, key,
            "Fluent key `{key}` is missing from locales/en-US/account.ftl (tr_args() fell back to the raw key)"
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

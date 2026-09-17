// SPDX-License-Identifier: MIT OR Apache-2.0

//! Every Fluent key the app shell, nav rail, notification area, and
//! Settings screens use (contracts/ui-surface.md "Main window",
//! "Notification message keys", "Settings") must resolve against
//! `locales/en-US/*.ftl` (research R10). Extended to cover every
//! Settings-screen key in US5 (T086), and every Now Playing / queue /
//! transfer-banner / stream-notification / device-name / "Play from
//! account" key added by 003-streaming-playback-and-queue (T064, T071,
//! T080, T095), including a check that `playback.ftl`/`settings.ftl`
//! define no key this test does not exercise.

use std::collections::HashSet;

use modplayer_core::{tr, tr_args};

/// Shell/nav/notification keys with no Fluent placeholder — resolved via
/// plain `tr`.
const SHELL_AND_NOTIFICATION_KEYS: &[&str] = &[
    // Nav rail.
    "nav-library",
    "nav-now-playing",
    "nav-plugins",
    "nav-settings",
    // Placeholder sections. `placeholder-library` was retired in
    // 004-search-and-library-browse (T026/T065: `library_view` replaces
    // the placeholder wiring point) — `SCAFFOLD_KEYS_MUST_BE_GONE` below
    // asserts it no longer resolves.
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

/// Now Playing status-line / stream-notification / transfer-banner /
/// queue keys with no Fluent placeholder (003-streaming-playback-and-queue,
/// T058/T064/T069/T071/T079/T080/T090; contracts/ui-surface.md §1/§2/§5) —
/// resolved via plain `tr`.
const PLAYBACK_KEYS: &[&str] = &[
    // Transport (now_playing.rs).
    "transport-skip-back",
    "transport-skip-forward",
    "now-playing-empty",
    // Status line.
    "status-buffering",
    "status-no-device",
    "status-premium-required",
    "status-subscription-not-verified",
    "status-signed-out",
    "status-not-registered",
    "status-source-unavailable",
    "status-reconnecting",
    // Transfer banner.
    "banner-play-here",
    "banner-unknown-device",
    // Queue panel.
    "queue-toggle",
    "queue-shuffle",
    "queue-repeat-off",
    "queue-repeat-one",
    "queue-repeat-all",
    "queue-current",
    "queue-badge-play-next",
    "queue-badge-unavailable",
    "queue-move-up",
    "queue-move-down",
    "queue-play-next",
    "queue-remove",
    "queue-empty",
    // Stream/session/subscription notifications and their actions.
    "transfer-request-failed",
    "stream-reconnect-warning",
    "stream-source-unavailable",
    "stream-source-update-required",
    "subscription-downgraded",
    "action-status-page",
    "action-retry",
    "action-open-upgrade-page",
];

/// Now Playing / queue / transfer-banner keys that take a Fluent
/// placeholder — resolved via `tr_args` with a stand-in value.
const PLAYBACK_ARG_KEYS: &[&str] = &[
    "transport-position",
    "now-playing-title",
    "now-playing-artist",
    "queue-row",
    "banner-playing-elsewhere",
];

/// Settings › Playback (device name) keys added by
/// 003-streaming-playback-and-queue (T062/T063/T064; contracts/ui-
/// surface.md §3/§5) — resolved via plain `tr`. Developer's "Play from
/// account" keys were retired alongside the scaffold itself in
/// 004-search-and-library-browse (T062) — `SCAFFOLD_KEYS_MUST_BE_GONE`
/// below asserts they no longer exist.
const PLAYBACK_SETTINGS_KEYS: &[&str] = &[
    "setting-device-name",
    "setting-device-name-hint",
    "setting-device-name-desc",
    "setting-device-name-too-long",
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

/// Search keys (US1, T040; contracts/ui-surface.md §2/§9) with no Fluent
/// placeholder — resolved via plain `tr`. `nav-search` lives in `app.ftl`
/// alongside the rest of the nav rail; the rest live in `library.ftl`.
/// `row-*`/`action-*`/`coming-soon`/`loading`/`detail-back` were already
/// seeded in Foundational (T027) and are not repeated here.
const SEARCH_KEYS: &[&str] = &[
    "nav-search",
    "search-placeholder",
    "search-offline",
    "search-group-tracks",
    "search-group-albums",
    "search-group-artists",
    "search-group-playlists",
    "refreshing",
];

/// Search keys that take a Fluent placeholder — resolved via `tr_args`.
const SEARCH_ARG_KEYS: &[&str] = &["search-no-results", "search-show-more"];

/// Library/detail keys (US2, T066; contracts/ui-surface.md §3/§4/§9) with
/// no Fluent placeholder — resolved via plain `tr`. `row-*`/`action-play-*`/
/// `action-add-*`/`action-save-to-library`/`action-pin-offline`/
/// `coming-soon`/`loading`/`detail-back` were already seeded in
/// Foundational (T027) and are exercised by `rows.rs`'s own tests, not
/// repeated here.
const LIBRARY_KEYS: &[&str] = &[
    "library-tab-saved-tracks",
    "library-tab-saved-albums",
    "library-tab-followed-artists",
    "library-tab-playlists",
    "library-tab-recently-played",
    "library-empty",
    "library-empty-albums",
    "library-empty-artists",
    "library-empty-playlists",
    "library-empty-recent",
    "library-first-sync-failed",
    "library-retry",
    "action-search",
    "action-create",
    "playlist-no-tracks",
];

/// Library keys that take a Fluent placeholder — resolved via `tr_args`.
const LIBRARY_ARG_KEYS: &[&str] = &["playlist-owner", "playlist-track-count"];

/// 003's "Play from account" scaffold keys (`developer.rs`'s doc comment,
/// FR-022) and `placeholder-library` (T026/T065) — retired alongside their
/// code paths in 004-search-and-library-browse. None of these may resolve
/// any more: a stray leftover key with no code path would otherwise
/// silently pass `no_unused_keys_in_playback_and_settings_ftl` simply by
/// never being tested, rather than by being absent from the `.ftl` files.
const SCAFFOLD_KEYS_MUST_BE_GONE: &[&str] = &[
    "placeholder-library",
    "setting-play-from-account",
    "setting-play-from-account-desc",
    "play-from-account-loading",
    "play-from-account-empty",
    "play-from-account-failed",
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

    for key in SEARCH_KEYS {
        let resolved = tr(key);
        assert_ne!(
            &resolved, key,
            "Fluent key `{key}` is missing from locales/en-US/*.ftl (tr() fell back to the raw key)"
        );
    }

    for key in SEARCH_ARG_KEYS {
        let resolved = tr_args(
            key,
            &[
                ("query", "abba".to_string()),
                ("group", "Tracks".to_string()),
            ],
        );
        assert_ne!(
            &resolved, key,
            "Fluent key `{key}` is missing from locales/en-US/library.ftl (tr_args() fell back to the raw key)"
        );
    }

    for key in LIBRARY_KEYS {
        let resolved = tr(key);
        assert_ne!(
            &resolved, key,
            "Fluent key `{key}` is missing from locales/en-US/library.ftl (tr() fell back to the raw key)"
        );
    }

    for key in LIBRARY_ARG_KEYS {
        let resolved = tr_args(
            key,
            &[("name", "Alex".to_string()), ("count", "3".to_string())],
        );
        assert_ne!(
            &resolved, key,
            "Fluent key `{key}` is missing from locales/en-US/library.ftl (tr_args() fell back to the raw key)"
        );
    }

    for key in PLAYBACK_KEYS {
        let resolved = tr(key);
        assert_ne!(
            &resolved, key,
            "Fluent key `{key}` is missing from locales/en-US/playback.ftl (tr() fell back to the raw key)"
        );
    }

    for key in PLAYBACK_SETTINGS_KEYS {
        let resolved = tr(key);
        assert_ne!(
            &resolved, key,
            "Fluent key `{key}` is missing from locales/en-US/settings.ftl (tr() fell back to the raw key)"
        );
    }

    for key in PLAYBACK_ARG_KEYS {
        let resolved = tr_args(
            key,
            &[
                ("position", "0:00".to_string()),
                ("duration", "3:45".to_string()),
                ("title", "Example Track".to_string()),
                ("artist", "Example Artist".to_string()),
                ("device", "Example Device".to_string()),
            ],
        );
        assert_ne!(
            &resolved, key,
            "Fluent key `{key}` is missing from locales/en-US/playback.ftl (tr_args() fell back to the raw key)"
        );
    }
}

/// Every message identifier a Fluent (`.ftl`) resource defines: lines of
/// the form `key = value`, skipping comments (`#`), section headers
/// (`##`/`###`), and blank lines. Good enough for `en-US`'s flat,
/// one-key-per-line resources — it does not need to understand Fluent's
/// full grammar, only to find identifiers at the start of a line.
fn defined_keys(ftl: &str) -> HashSet<&str> {
    ftl.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            line.split_once(" = ").map(|(key, _)| key.trim())
        })
        .collect()
}

/// FR-024 / T095: every key `playback.ftl` and `settings.ftl` define is
/// exercised by this file — no orphaned translation the app never
/// displays. (Coverage the other direction — every key the app looks up
/// exists — is `every_shell_nav_and_notification_key_resolves` above.)
#[test]
fn no_unused_keys_in_playback_and_settings_ftl() {
    let playback_ftl = include_str!("../../../locales/en-US/playback.ftl");
    let settings_ftl = include_str!("../../../locales/en-US/settings.ftl");

    let tested: HashSet<&str> = SHELL_AND_NOTIFICATION_KEYS
        .iter()
        .chain(SETTINGS_SCREEN_KEYS)
        .chain(ACCOUNT_KEYS)
        .chain(PLAYBACK_KEYS)
        .chain(PLAYBACK_ARG_KEYS)
        .chain(PLAYBACK_SETTINGS_KEYS)
        .copied()
        .collect();

    for key in defined_keys(playback_ftl) {
        assert!(
            tested.contains(key),
            "locales/en-US/playback.ftl defines `{key}`, which no UI code path (and so no test in this file) uses — remove it or wire it up"
        );
    }
    for key in defined_keys(settings_ftl) {
        assert!(
            tested.contains(key),
            "locales/en-US/settings.ftl defines `{key}`, which no UI code path (and so no test in this file) uses — remove it or wire it up"
        );
    }
}

/// FR-022/T062, T026/T065: the 003 "Play from account" scaffold and the
/// `placeholder-library` wiring point are gone, not merely unused — none
/// of their keys resolve, and none of the three `.ftl` files this test
/// suite covers still define them.
#[test]
fn scaffold_keys_no_longer_resolve_or_exist() {
    for key in SCAFFOLD_KEYS_MUST_BE_GONE {
        let resolved = tr(key);
        assert_eq!(
            &resolved, key,
            "`{key}` still resolves to a real string — the 003 scaffold (or \
             its `.ftl` key) was expected gone as of 004-search-and-library-browse"
        );
    }

    let app_ftl = include_str!("../../../locales/en-US/app.ftl");
    let playback_ftl = include_str!("../../../locales/en-US/playback.ftl");
    let settings_ftl = include_str!("../../../locales/en-US/settings.ftl");
    let defined: HashSet<&str> = defined_keys(app_ftl)
        .into_iter()
        .chain(defined_keys(playback_ftl))
        .chain(defined_keys(settings_ftl))
        .collect();
    for key in SCAFFOLD_KEYS_MUST_BE_GONE {
        assert!(
            !defined.contains(key),
            "`{key}` is still defined in app.ftl/playback.ftl/settings.ftl — remove it with the scaffold"
        );
    }
}

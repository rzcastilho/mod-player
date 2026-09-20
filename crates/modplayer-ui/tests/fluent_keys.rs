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
    // 005-now-playing-waveform: waveform overview/detail (contracts/
    // ui-waveform.md §1).
    "transport-seek",
    "now-playing-pick-a-track",
    "waveform-unavailable",
    "waveform-detail",
    "waveform-overview-desc",
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
    // Markers and loop regions (006, US1; contracts/ui-markers.md §8):
    // the panel/empty state, view-level shortcut refusals, the lane's
    // region role labels, and the panel's loop-only cells. The contract
    // also lists a separate `markers-heading` key for the panel header's
    // own label; `markers.rs::panel` consolidates it into `markers-panel`
    // (one key doing double duty as both the panel's own accessible name,
    // contracts §1, and its heading text, contracts §4) rather than
    // defining an unused second key — same pattern as T076's documented
    // colour-swatch simplification. `markers-heading` is asserted absent
    // below (`markers_heading_key_was_intentionally_consolidated`) so this
    // deviation stays pinned, not silently forgotten.
    "markers-panel",
    "markers-empty",
    "markers-status",
    "marker-limit-reached",
    "loop-region-incomplete",
    "loop-region-too-short",
    "marker-role-a",
    "marker-role-b",
    "loop-arm",
    "loop-disarm",
    "loop-repeat",
    "loop-crossfade",
    "loop-wraps-infinite",
    "loop-armed-inactive",
    // 006 US2: panel header (new-region/clear-all) and persistence
    // warnings (contracts/marker-service.md §6, contracts/ui-markers.md
    // §4/§8).
    "markers-new-loop",
    "markers-clear-all",
    "markers-clear-yes",
    "markers-clear-no",
    "marker-clamped-desc",
    "track-state-unreadable",
    "track-state-newer-version",
    "track-state-save-failed",
    // 006 US3: precise marker editing (contracts/ui-markers.md §3/§4/§8) —
    // the point-kind role label and the inline-rename field's accessible
    // name.
    "marker-role-point",
    "markers-rename",
];

/// Now Playing / queue / transfer-banner keys that take a Fluent
/// placeholder — resolved via `tr_args` with a stand-in value.
const PLAYBACK_ARG_KEYS: &[&str] = &[
    "now-playing-title",
    "now-playing-artist",
    "now-playing-album",
    "queue-row",
    "banner-playing-elsewhere",
];

/// 005-now-playing-waveform's waveform-detail-window key: templated with
/// `$start`/`$end` (contracts/ui-waveform.md §1). Used by the detail
/// widget's own accessible description (US2, T043) and by
/// `waveform-detail-window`'s own fluent_keys coverage here regardless,
/// since the key already exists in `playback.ftl` (T004).
const PLAYBACK_WINDOW_ARG_KEYS: &[&str] = &["waveform-detail-window"];

/// 005-now-playing-waveform's `time-elapsed`/`time-remaining`: templated
/// with `$time` (an already-formatted `m:ss` string).
const PLAYBACK_TIME_ARG_KEYS: &[&str] = &["time-elapsed", "time-remaining"];

/// 006 US1's `loop-wraps-remaining` and US2's `markers-clear-confirm`:
/// both templated with `$count`.
const PLAYBACK_COUNT_ARG_KEYS: &[&str] = &["loop-wraps-remaining", "markers-clear-confirm"];

/// 006's `marker-glyph`: templated with `$role`/`$name`/`$time`
/// (contracts/ui-markers.md §3).
const PLAYBACK_MARKER_GLYPH_KEYS: &[&str] = &["marker-glyph"];

/// 006 US3's `marker-default-name`: templated with `$n` (data-model.md
/// §1.3, design note 14 — the core model resolves it via `tr_args` even
/// though it stores the resolved string).
const PLAYBACK_DEFAULT_NAME_ARG_KEYS: &[&str] = &["marker-default-name"];

/// 006 US3's `markers-color`: templated with `$index` (contracts/
/// ui-markers.md §4's colour-swatch cell).
const PLAYBACK_INDEX_ARG_KEYS: &[&str] = &["markers-color"];

/// 006 US4's `marker-role-cue`: templated with `$slot` (contracts/
/// ui-markers.md §3/§4 — the cue glyph/row role label).
const PLAYBACK_SLOT_ARG_KEYS: &[&str] = &["marker-role-cue"];

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
    // 006 US3 (contracts/ui-markers.md §7): the marker nudge-step field.
    "setting-nudge-step",
    "setting-nudge-step-desc",
];

/// 007-keyboard-actions-and-shortcuts, T068: the 44 action labels and 6
/// category labels (`controls.ftl`, contracts/ui-actions.md §6), plus the
/// Controls page's own strings that take no Fluent placeholder — resolved
/// via plain `tr`.
const CONTROLS_KEYS: &[&str] = &[
    "action-transport-play",
    "action-transport-pause",
    "action-transport-toggle",
    "action-transport-stop",
    "action-transport-next",
    "action-transport-previous",
    "action-transport-seek-forward-step",
    "action-transport-seek-backward-step",
    "action-transport-volume-up",
    "action-transport-volume-down",
    "action-markers-add-point",
    "action-markers-set-a",
    "action-markers-set-b",
    "action-markers-nudge-earlier",
    "action-markers-nudge-later",
    "action-markers-nudge-earlier-x10",
    "action-markers-nudge-later-x10",
    "action-markers-clear-all",
    "action-loop-toggle",
    "action-cues-set-1",
    "action-cues-set-2",
    "action-cues-set-3",
    "action-cues-set-4",
    "action-cues-set-5",
    "action-cues-set-6",
    "action-cues-set-7",
    "action-cues-set-8",
    "action-cues-jump-1",
    "action-cues-jump-2",
    "action-cues-jump-3",
    "action-cues-jump-4",
    "action-cues-jump-5",
    "action-cues-jump-6",
    "action-cues-jump-7",
    "action-cues-jump-8",
    "action-nav-library",
    "action-nav-search",
    "action-nav-now-playing",
    "action-nav-plugins",
    "action-nav-settings",
    "action-nav-toggle-queue",
    "action-nav-focus-search",
    "action-effects-tempo-step-up",
    "action-effects-tempo-step-down",
    // 008, contracts/effects-service.md §4.
    "action-nav-toggle-effect-chain",
    "action-cat-transport",
    "action-cat-markers",
    "action-cat-loop",
    "action-cat-cues",
    "action-cat-navigation",
    "action-cat-effects",
    "controls-heading",
    "controls-filter",
    "controls-no-match",
    "controls-kind-trigger",
    "controls-kind-continuous",
    "controls-inactive",
    "controls-add-binding",
    "controls-capture-prompt",
    "controls-reset-all",
    "controls-reset-all-confirm",
    "controls-confirm",
    "controls-cancel",
    "controls-reject-tab",
    "controls-reject-mac-control",
    "controls-reject-modifier-only",
    "controls-reject-duplicate",
];

/// `controls.ftl` keys that take a Fluent placeholder — resolved via
/// `tr_args` with a stand-in value.
const CONTROLS_ARG_KEYS: &[&str] = &[
    "controls-binding-chip",
    "controls-remove-binding",
    "controls-capture",
    "controls-reset-action",
    "controls-conflict-with",
];

/// `controls.ftl`'s one new settings-load warning (FR-013;
/// contracts/keymap-settings.md), templated with `{ $ids }` — resolved via
/// `tr_args` like `DEVICE_NAMED_KEYS`.
const CONTROLS_WARNING_ARG_KEYS: &[&str] = &["keybindings-invalid-entries"];

/// `effects.ftl` keys added by 008 Phase 4 (US2), Phase 5 (US3) and
/// Phase 6 (US4; contracts/ui-effect-chain.md §6) with no Fluent
/// placeholder — resolved via plain `tr`.
const EFFECTS_KEYS: &[&str] = &[
    "effects-toggle",
    "effects-panel-title",
    "effects-add-node",
    "effects-add",
    "effects-remove",
    "effects-bypass",
    "effects-reorder-handle",
    "effects-mode-note",
    "effects-chain-full",
    "effects-owner-host",
    "effects-owner-plugin",
    "effects-kind-pitch-shift",
    "effects-kind-time-stretch",
    "effects-kind-gain",
    "effects-param-semitones",
    "effects-param-formant",
    "effects-param-mode",
    "effects-param-ratio",
    "effects-param-level",
    "effects-param-mute",
    "effects-mode-performance",
    "effects-mode-quality",
    // Phase 5 (US3): equalizer, filter, stereo tools.
    "effects-kind-equalizer",
    "effects-kind-filter",
    "effects-kind-stereo-tools",
    "effects-param-band-freq",
    "effects-param-band-gain",
    "effects-param-band-q",
    "effects-param-band-type",
    "effects-param-filter-mode",
    "effects-param-cutoff",
    "effects-param-resonance",
    "effects-param-width",
    "effects-param-balance",
    "effects-param-mono-sum",
    "effects-param-phase-invert",
    "effects-param-channel-swap",
    "effects-band-type-peak",
    "effects-band-type-low-shelf",
    "effects-band-type-high-shelf",
    "effects-filter-high-pass",
    "effects-filter-low-pass",
    // Phase 6 (US4): meters, spectrum, overload/auto-bypass.
    "effects-over-budget-badge",
    "effects-pre",
    "effects-post",
    "effects-peak",
    "effects-rms",
    "effects-spectrum",
    "effects-auto-bypassed",
];

/// `effects.ftl` keys templated with `{ $pct }` — resolved via `tr_args`.
const EFFECTS_PCT_ARG_KEYS: &[&str] = &["effects-chain-cpu", "effects-cpu"];

/// `effects.ftl` keys templated with `{ $count }` — resolved via
/// `tr_args` (008 Phase 6, contracts/ui-effect-chain.md §2's overload
/// counter).
const EFFECTS_COUNT_ARG_KEYS: &[&str] = &["effects-overloads"];

/// `effects.ftl` notification keys (008, data-model.md §6): `{ $node }`,
/// `effect-chain-over-budget` also `{ $owner }` — resolved via
/// `tr_args` from `PlaybackController::drain_engine_events`/`tempo_step`,
/// never directly by a widget in this file, but still owned by this
/// crate's `effects.ftl` (contracts/ui-effect-chain.md §6).
const EFFECTS_NODE_ARG_KEYS: &[&str] = &["effect-chain-auto-bypassed"];
const EFFECTS_NODE_OWNER_ARG_KEYS: &[&str] = &["effect-chain-over-budget"];
/// `effects-no-time-stretch` (008, Phase 3) takes no Fluent placeholder.
const EFFECTS_NO_ARG_NOTIFICATION_KEYS: &[&str] = &["effects-no-time-stretch"];

/// `plugins.ftl` keys with no Fluent placeholder (009-plugin-runtime-and-
/// permissions US4, T111; contracts/ui-plugins.md §2/§3): the section's own
/// chrome, the 25-entry permission catalog's explanation strings
/// (`Permission::explanation_key`, `plugins_view.rs::permissions_summary`
/// — any of the 25 can appear in a row's summary, not only the 9 operable
/// this slice, since a manifest may request any catalog permission), and
/// the suspension-cause fragments `plugins/host.rs::cause_label` resolves.
/// `manifest-error-*` is deliberately **not** listed here:
/// `ManifestError`'s `Display` (`crates/modplayer-capability-gateway/src/
/// manifest.rs`) renders its own hard-coded English sentence — kept in
/// sync with these Fluent strings by contracts/manifest.md §3's ordering
/// alone, never looked up via `tr`/`tr_args` — so those 8 keys exist in
/// `plugins.ftl` for parity/future localization but are not exercised by
/// any UI code path this test could assert against.
const PLUGINS_KEYS: &[&str] = &[
    "plugins-title",
    "plugins-empty",
    "plugins-col-name",
    "plugins-col-version",
    "plugins-col-source",
    "plugins-col-enabled",
    "plugins-col-health",
    "plugins-col-permissions",
    "plugins-col-cpu",
    "plugins-col-memory",
    "plugins-source-bundled",
    "plugins-health-ok",
    "plugins-health-warning",
    "plugins-health-suspended",
    "plugins-dash",
    "plugins-list-separator",
    "permission-playback-observe",
    "permission-transport-control",
    "permission-queue-write",
    "permission-markers-read",
    "permission-markers-write",
    "permission-audio-effects",
    "permission-audio-meter",
    "permission-audio-process",
    "permission-analysis-read",
    "permission-analysis-write",
    "permission-library-read",
    "permission-library-write",
    "permission-ui-panel",
    "permission-ui-overlay",
    "permission-ui-shortcuts",
    "permission-ui-notify",
    "permission-ui-settings",
    "permission-state-plugin",
    "permission-state-track",
    "permission-network",
    "permission-files-read",
    "permission-files-write",
    "permission-midi-observe",
    "permission-midi-output",
    "permission-clipboard",
    "plugin-suspended-cause-hang",
    "plugin-suspended-cause-cpu-share",
    "plugin-suspended-cause-memory",
    "plugin-suspended-cause-did-not-start",
    "notification-action-restart-plugin",
    "notification-action-disable-plugin",
];

/// `plugins.ftl` keys that take a Fluent placeholder — resolved via
/// `tr_args` with a stand-in value.
const PLUGINS_ARG_KEYS: &[&str] = &[
    "plugins-enable-toggle",
    "plugins-invalid-manifest",
    "plugins-cpu",
    "plugins-memory",
    "plugin-suspended",
    "plugin-auto-disabled",
];

/// Every Settings-screen key (US5, T086): the eleven fixed-order category
/// labels, the search box, the placeholder text for categories with no
/// working settings yet, and every working setting's title/description
/// (contracts/ui-surface.md "Settings"). 007-keyboard-actions-and-shortcuts
/// (T068) adds the Controls category's one descriptor,
/// `setting-keybindings`/`setting-keybindings-desc`.
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
    // Controls category (007-keyboard-actions-and-shortcuts, T061/T068).
    "setting-keybindings",
    "setting-keybindings-desc",
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
/// FR-022), `placeholder-library` (T026/T065), and `placeholder-plugins`
/// (009-plugin-runtime-and-permissions T106/T107: `plugins_view::show`
/// replaces `shell::plugins_placeholder`) — retired alongside their code
/// paths. None of these may resolve any more: a stray leftover key with no
/// code path would otherwise silently pass
/// `no_unused_keys_in_playback_and_settings_ftl` simply by never being
/// tested, rather than by being absent from the `.ftl` files.
const SCAFFOLD_KEYS_MUST_BE_GONE: &[&str] = &[
    "placeholder-library",
    "placeholder-plugins",
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
                ("title", "Example Track".to_string()),
                ("artist", "Example Artist".to_string()),
                ("album", "Example Album".to_string()),
                ("device", "Example Device".to_string()),
            ],
        );
        assert_ne!(
            &resolved, key,
            "Fluent key `{key}` is missing from locales/en-US/playback.ftl (tr_args() fell back to the raw key)"
        );
    }

    for key in PLAYBACK_WINDOW_ARG_KEYS {
        let resolved = tr_args(
            key,
            &[("start", "1:10".to_string()), ("end", "1:40".to_string())],
        );
        assert_ne!(
            &resolved, key,
            "Fluent key `{key}` is missing from locales/en-US/playback.ftl (tr_args() fell back to the raw key)"
        );
    }

    for key in PLAYBACK_TIME_ARG_KEYS {
        let resolved = tr_args(key, &[("time", "1:23".to_string())]);
        assert_ne!(
            &resolved, key,
            "Fluent key `{key}` is missing from locales/en-US/playback.ftl (tr_args() fell back to the raw key)"
        );
    }

    for key in PLAYBACK_COUNT_ARG_KEYS {
        let resolved = tr_args(key, &[("count", "3".to_string())]);
        assert_ne!(
            &resolved, key,
            "Fluent key `{key}` is missing from locales/en-US/playback.ftl (tr_args() fell back to the raw key)"
        );
    }

    for key in PLAYBACK_MARKER_GLYPH_KEYS {
        let resolved = tr_args(
            key,
            &[
                ("role", "A".to_string()),
                ("name", "Marker 1".to_string()),
                ("time", "1:23".to_string()),
            ],
        );
        assert_ne!(
            &resolved, key,
            "Fluent key `{key}` is missing from locales/en-US/playback.ftl (tr_args() fell back to the raw key)"
        );
    }

    for key in PLAYBACK_DEFAULT_NAME_ARG_KEYS {
        let resolved = tr_args(key, &[("n", "1".to_string())]);
        assert_ne!(
            &resolved, key,
            "Fluent key `{key}` is missing from locales/en-US/playback.ftl (tr_args() fell back to the raw key)"
        );
    }

    for key in PLAYBACK_INDEX_ARG_KEYS {
        let resolved = tr_args(key, &[("index", "0".to_string())]);
        assert_ne!(
            &resolved, key,
            "Fluent key `{key}` is missing from locales/en-US/playback.ftl (tr_args() fell back to the raw key)"
        );
    }

    for key in PLAYBACK_SLOT_ARG_KEYS {
        let resolved = tr_args(key, &[("slot", "1".to_string())]);
        assert_ne!(
            &resolved, key,
            "Fluent key `{key}` is missing from locales/en-US/playback.ftl (tr_args() fell back to the raw key)"
        );
    }

    for key in CONTROLS_KEYS {
        let resolved = tr(key);
        assert_ne!(
            &resolved, key,
            "Fluent key `{key}` is missing from locales/en-US/controls.ftl (tr() fell back to the raw key)"
        );
    }

    for key in CONTROLS_ARG_KEYS {
        let resolved = tr_args(
            key,
            &[
                ("binding", "⌘⇧→".to_string()),
                ("action", "Play/pause".to_string()),
                ("other", "Stop".to_string()),
            ],
        );
        assert_ne!(
            &resolved, key,
            "Fluent key `{key}` is missing from locales/en-US/controls.ftl (tr_args() fell back to the raw key)"
        );
    }

    for key in CONTROLS_WARNING_ARG_KEYS {
        let resolved = tr_args(key, &[("ids", "host.nope.x".to_string())]);
        assert_ne!(
            &resolved, key,
            "Fluent key `{key}` is missing from locales/en-US/controls.ftl (tr_args() fell back to the raw key)"
        );
    }

    for key in EFFECTS_KEYS {
        let resolved = tr(key);
        assert_ne!(
            &resolved, key,
            "Fluent key `{key}` is missing from locales/en-US/effects.ftl (tr() fell back to the raw key)"
        );
    }

    for key in EFFECTS_PCT_ARG_KEYS {
        let resolved = tr_args(key, &[("pct", "12".to_string())]);
        assert_ne!(
            &resolved, key,
            "Fluent key `{key}` is missing from locales/en-US/effects.ftl (tr_args() fell back to the raw key)"
        );
    }

    for key in EFFECTS_COUNT_ARG_KEYS {
        let resolved = tr_args(key, &[("count", "3".to_string())]);
        assert_ne!(
            &resolved, key,
            "Fluent key `{key}` is missing from locales/en-US/effects.ftl (tr_args() fell back to the raw key)"
        );
    }

    for key in EFFECTS_NODE_ARG_KEYS {
        let resolved = tr_args(key, &[("node", "Gain".to_string())]);
        assert_ne!(
            &resolved, key,
            "Fluent key `{key}` is missing from locales/en-US/effects.ftl (tr_args() fell back to the raw key)"
        );
    }

    for key in EFFECTS_NODE_OWNER_ARG_KEYS {
        let resolved = tr_args(
            key,
            &[
                ("node", "Gain".to_string()),
                ("owner", "plugin".to_string()),
            ],
        );
        assert_ne!(
            &resolved, key,
            "Fluent key `{key}` is missing from locales/en-US/effects.ftl (tr_args() fell back to the raw key)"
        );
    }

    for key in EFFECTS_NO_ARG_NOTIFICATION_KEYS {
        let resolved = tr(key);
        assert_ne!(
            &resolved, key,
            "Fluent key `{key}` is missing from locales/en-US/effects.ftl (tr() fell back to the raw key)"
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

/// 009-plugin-runtime-and-permissions US4 (T111, contracts/ui-plugins.md
/// §2/§3): every `plugins.ftl` key `plugins_view.rs`/`notifications.rs`/
/// `plugins/host.rs` actually look up resolves to a real string.
#[test]
fn plugins_ftl_keys_used_exist() {
    for key in PLUGINS_KEYS {
        let resolved = tr(key);
        assert_ne!(
            &resolved, key,
            "Fluent key `{key}` is missing from locales/en-US/plugins.ftl (tr() fell back to the raw key)"
        );
    }

    for key in PLUGINS_ARG_KEYS {
        let resolved = tr_args(
            key,
            &[
                ("plugin", "Example plugin".to_string()),
                ("reason", "example reason".to_string()),
                ("pct", "3".to_string()),
                ("used", "1.2".to_string()),
                ("cause", "it stopped responding".to_string()),
            ],
        );
        assert_ne!(
            &resolved, key,
            "Fluent key `{key}` is missing from locales/en-US/plugins.ftl (tr_args() fell back to the raw key)"
        );
    }
}

/// FR-024 / T095: every key `playback.ftl` and `settings.ftl` define is
/// exercised by this file — no orphaned translation the app never
/// displays. (Coverage the other direction — every key the app looks up
/// exists — is `every_shell_nav_and_notification_key_resolves` above.)
#[test]
fn no_unused_keys_in_playback_and_settings_ftl() {
    let playback_ftl = include_str!("../../../locales/en-US/playback.ftl");
    let settings_ftl = include_str!("../../../locales/en-US/settings.ftl");
    let controls_ftl = include_str!("../../../locales/en-US/controls.ftl");
    let effects_ftl = include_str!("../../../locales/en-US/effects.ftl");

    let tested: HashSet<&str> = SHELL_AND_NOTIFICATION_KEYS
        .iter()
        .chain(SETTINGS_SCREEN_KEYS)
        .chain(ACCOUNT_KEYS)
        .chain(PLAYBACK_KEYS)
        .chain(PLAYBACK_ARG_KEYS)
        .chain(PLAYBACK_WINDOW_ARG_KEYS)
        .chain(PLAYBACK_TIME_ARG_KEYS)
        .chain(PLAYBACK_COUNT_ARG_KEYS)
        .chain(PLAYBACK_MARKER_GLYPH_KEYS)
        .chain(PLAYBACK_DEFAULT_NAME_ARG_KEYS)
        .chain(PLAYBACK_INDEX_ARG_KEYS)
        .chain(PLAYBACK_SLOT_ARG_KEYS)
        .chain(PLAYBACK_SETTINGS_KEYS)
        .chain(CONTROLS_KEYS)
        .chain(CONTROLS_ARG_KEYS)
        .chain(CONTROLS_WARNING_ARG_KEYS)
        .chain(EFFECTS_KEYS)
        .chain(EFFECTS_PCT_ARG_KEYS)
        .chain(EFFECTS_COUNT_ARG_KEYS)
        .chain(EFFECTS_NODE_ARG_KEYS)
        .chain(EFFECTS_NODE_OWNER_ARG_KEYS)
        .chain(EFFECTS_NO_ARG_NOTIFICATION_KEYS)
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
    for key in defined_keys(controls_ftl) {
        assert!(
            tested.contains(key),
            "locales/en-US/controls.ftl defines `{key}`, which no UI code path (and so no test in this file) uses — remove it or wire it up"
        );
    }
    for key in defined_keys(effects_ftl) {
        assert!(
            tested.contains(key),
            "locales/en-US/effects.ftl defines `{key}`, which no UI code path (and so no test in this file) uses — remove it or wire it up"
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

/// T090 (Phase 7, FR-023): contracts/ui-markers.md §8 lists `markers-heading`
/// alongside `markers-panel` as two separate keys, but `markers.rs::panel`
/// consolidates both into the one `markers-panel` key (documented above,
/// `PLAYBACK_KEYS`). Pins that consolidation deliberately: `markers-heading`
/// must never resolve and must never be (re-)defined, so a future edit that
/// adds it back either updates this test and the comment together or is
/// caught here as an unreviewed drift from the documented deviation.
#[test]
fn markers_heading_key_was_intentionally_consolidated() {
    let resolved = tr("markers-heading");
    assert_eq!(
        &resolved, "markers-heading",
        "`markers-heading` resolves to a real string, but `markers.rs::panel` was expected \
         to use `markers-panel` alone for the header (contracts/ui-markers.md §8's \
         documented consolidation) — update the `PLAYBACK_KEYS` comment if this is now \
         intentional, or remove the new definition if it is not"
    );

    let playback_ftl = include_str!("../../../locales/en-US/playback.ftl");
    assert!(
        !defined_keys(playback_ftl).contains("markers-heading"),
        "`markers-heading` is now defined in playback.ftl — either wire it up and add it to \
         `PLAYBACK_KEYS` above, or this test/comment is stale"
    );
}

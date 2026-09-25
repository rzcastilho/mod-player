// SPDX-License-Identifier: MIT OR Apache-2.0

//! Notifications: severity-classified, newest-first, Fluent-keyed messages
//! (data-model.md §5.3, §6.4; FR-016). `Info` auto-dismisses at 10 s;
//! `Warning`/`Critical` persist until manually dismissed.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use modplayer_effects::catalog::PluginId;

/// How long an `Info` notification stays visible before auto-dismissing.
pub const INFO_AUTO_DISMISS: Duration = Duration::from_secs(10);

/// Fluent message keys for device-lifecycle notifications (US3, spec
/// acceptance 1-5; data-model.md §6.2). Named here so `controller.rs` and
/// `device_policy.rs` raise them by constant rather than scattering string
/// literals; locale content lives in `locales/en-US/app.ftl`.
pub const KEY_DEVICE_LOST: &str = "device-lost";
pub const KEY_DEVICE_MISSING_AT_LAUNCH: &str = "device-missing-at-launch";
pub const KEY_DEVICE_AVAILABLE_AGAIN: &str = "device-available-again";
pub const KEY_NO_OUTPUT_DEVICES: &str = "no-output-devices";
pub const KEY_DEVICE_APPEARED: &str = "device-appeared";

/// FR-026: a queue item the source reported unavailable was skipped
/// (003-streaming-playback-and-queue, contracts/transport-and-queue.md §2
/// rule T14, data-model.md §9). `{ $title }`.
pub const KEY_QUEUE_ITEM_SKIPPED_UNAVAILABLE: &str = "queue-item-skipped-unavailable";

/// FR-018: the 5 s "take over playback here" request timed out with no
/// `BecameActive` from the service (003-streaming-playback-and-queue,
/// contracts/transport-and-queue.md §2 rule T19, data-model.md §9).
pub const KEY_TRANSFER_REQUEST_FAILED: &str = "transfer-request-failed";

/// FR-021/SC-009: a transient source failure has not recovered within
/// 30 s (003-streaming-playback-and-queue, contracts/transport-and-
/// queue.md §2 rule T20, data-model.md §9). Auto-cleared on `Health(Ok)`.
pub const KEY_STREAM_RECONNECT_WARNING: &str = "stream-reconnect-warning";

/// FR-020/SC-009: the source is unavailable and no client update is
/// required — critical, with **Open status page** + **Retry** actions
/// (rule T21, data-model.md §9).
pub const KEY_STREAM_SOURCE_UNAVAILABLE: &str = "stream-source-unavailable";

/// FR-020/SC-009: the source is unavailable because this client's
/// protocol version was rejected — critical, with **Open status page**
/// only (rule T21, data-model.md §9).
pub const KEY_STREAM_SOURCE_UPDATE_REQUIRED: &str = "stream-source-update-required";

/// FR-027/SC-013: the account tier was rejected/downgraded after the
/// current track finished (rule T22, data-model.md §9), with an **Open
/// upgrade page** action.
pub const KEY_SUBSCRIPTION_DOWNGRADED: &str = "subscription-downgraded";

/// FR-016/FR-017: the per-track marker/loop state file was corrupt,
/// oversized, or otherwise unreadable and was loaded as empty
/// (006-markers-loops-and-cues, data-model.md §6, contracts/
/// marker-service.md §6). Rewrite is allowed on the next mutation.
pub const KEY_TRACK_STATE_UNREADABLE: &str = "track-state-unreadable";

/// FR-016/FR-017: the per-track marker/loop state file's
/// `schema_version` is newer than this build understands
/// (006-markers-loops-and-cues, data-model.md §6). Loaded as empty; the
/// file is not rewritten until the user next mutates the state.
pub const KEY_TRACK_STATE_NEWER_VERSION: &str = "track-state-newer-version";

/// FR-017: the writer thread failed to save the per-track marker/loop
/// state (006-markers-loops-and-cues, data-model.md §6, contracts/
/// marker-service.md §6); the previous file, if any, is left intact and
/// there is no automatic retry.
pub const KEY_TRACK_STATE_SAVE_FAILED: &str = "track-state-save-failed";

/// FR-013/FR-015: one or more `[keybindings]` entries were dropped in
/// isolation at load (007, contracts/keymap-settings.md): an unknown
/// action id, a value that isn't an array of strings, or an unparseable
/// chord string. `{ $ids }`.
pub const KEY_KEYBINDINGS_INVALID_ENTRIES: &str = "keybindings-invalid-entries";

/// FR-017: `tempo_step` had no `TimeStretch` node to target (008,
/// contracts/effects-service.md §2 rule C3) — coalesced: raised only
/// while not already visible.
pub const KEY_EFFECTS_NO_TIME_STRETCH: &str = "effects-no-time-stretch";

/// FR-012, research R10: the chain's whole-render cost crossed the
/// overload threshold (008, contracts/effects-service.md §2 rule C5).
/// `{ $node }` (the costliest node's kind label), `{ $owner }`. Raised
/// only while not already visible; re-used (not re-raised) while the
/// excursion continues; dismissed by key once `RtShared::over_budget()`
/// clears (rule C6).
pub const KEY_EFFECT_CHAIN_OVER_BUDGET: &str = "effect-chain-over-budget";

/// FR-012: a non-host node was auto-bypassed after an overload event
/// named it the costliest active slot (008, contracts/effects-service.md
/// §2 rule C5). `{ $node }`. Never raised by any controller path this
/// slice's UI can reach (no plugin runtime yet) — exercised by engine
/// tests ahead of 009.
pub const KEY_EFFECT_CHAIN_AUTO_BYPASSED: &str = "effect-chain-auto-bypassed";

/// FR-011: a plugin was suspended for a fault (009, contracts/
/// plugin-host-service.md §5). `{ $plugin }` (name), `{ $cause }`
/// (localised cause); Restart + Disable actions; deduped by
/// `"plugin-suspended:<identifier>"` so a repeat suspension replaces
/// rather than stacks, and it is dismissed on the next successful
/// `ready()` (L4).
pub const KEY_PLUGIN_SUSPENDED: &str = "plugin-suspended";

/// FR-011: the third suspension within a session auto-disabled the
/// plugin (009, contracts/plugin-host-service.md §5). `{ $plugin }`; no
/// actions; deduped by `"plugin-auto-disabled:<identifier>"`, replacing
/// that plugin's `plugin-suspended` notice.
pub const KEY_PLUGIN_AUTO_DISABLED: &str = "plugin-auto-disabled";

/// Notification severity (data-model.md §6.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Critical,
    Warning,
    Info,
}

/// A notification's plugin origin (011-plugin-ui-contributions, US5,
/// contracts/overlays-settings-notify.md §3 N2): `id` for routing (e.g. a
/// future per-plugin mute), `name` the manifest display name already
/// resolved at raise time (mirrors `plugin-suspended`'s own
/// `plugin_display_name`), so a later disable/suspend of the plugin never
/// changes an already-shown notification's attribution (N4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginAttribution {
    pub id: PluginId,
    pub name: String,
}

/// An action a notification's button performs when clicked
/// (002-first-launch-and-sign-in contracts/account-session.md "Events",
/// contracts/ui-surface.md; 003-streaming-playback-and-queue data-model.md
/// §9). `SignIn` navigates to the sign-in step (e.g. `RefreshFailing`'s
/// `signin-again`, `SessionExpired`, `SessionRevoked`). `OpenStatusPage`/
/// `OpenUpgradePage` open `links::STATUS_PAGE_URL`/`modplayer_account::
/// UPGRADE_URL` (UI-only, design note 10); `RetrySource` sends
/// `SourceCommand::Retry`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationAction {
    SignIn,
    OpenStatusPage,
    RetrySource,
    OpenUpgradePage,
    /// `plugin-suspended`'s "Restart" (009, contracts/plugin-host-
    /// service.md §5): calls the façade's `plugin_restart(id)`.
    RestartPlugin(PluginId),
    /// `plugin-suspended`'s "Disable" (009): calls `plugin_disable(id)`.
    DisablePlugin(PluginId),
}

/// A single raised notification. `message_key` is a Fluent key, never raw
/// text (FR-021); `args` are Fluent placeholder values (e.g. `{ $device }`).
/// `action`, when set, is rendered as an extra button alongside Dismiss
/// (e.g. `session-expired`'s "Sign in"); `actions` is the same list in
/// full (data-model.md §9: up to 2, e.g. `stream-source-unavailable`'s
/// "Open status page" + "Retry") — `action` is always `actions.first()`,
/// kept for the UI code that only ever rendered a single button.
#[derive(Debug, Clone)]
pub struct Notification {
    pub id: u64,
    pub severity: Severity,
    pub message_key: &'static str,
    pub args: Vec<(&'static str, String)>,
    pub action: Option<NotificationAction>,
    pub actions: Vec<NotificationAction>,
    pub created_at: Instant,
    pub dismissed: bool,
    /// A stable key a later raise or explicit dismiss can target (009
    /// `raise_keyed`/`dismiss_by_dedupe`, e.g.
    /// `"plugin-suspended:<identifier>"`); `None` for every notification
    /// raised through the older `raise*` methods.
    pub dedupe_key: Option<String>,
    /// This notification's plugin origin (US5 N2), `None` for every
    /// host-raised notification (every `raise*` method above).
    pub attribution: Option<PluginAttribution>,
    /// Non-localised technical text shown behind "Details" (019-
    /// notification-presentation, FR-013, data-model.md §1). `None` unless
    /// raised via [`NotificationCenter::raise_with_detail`]; never set by
    /// `raise_attributed` (plugins cannot set it — Principle IX). When
    /// `Some`, always non-empty.
    pub detail: Option<String>,
    /// The origin of the `Info` auto-dismiss timer (019, data-model.md
    /// §1): `= created_at` at raise, reset to "now" by
    /// [`NotificationCenter::release_auto_dismiss`]. `created_at` itself
    /// stays the truthful "raised at" timestamp with its other readers
    /// untouched.
    pub(crate) auto_dismiss_from: Instant,
    /// `true` while the UI reports this card's "Show more" or "Details"
    /// expanded (019, FR-009): held `Info` notifications never auto-dismiss
    /// (WCAG 2.2.1).
    pub(crate) held: bool,
}

/// The most action buttons a single notification renders alongside
/// Dismiss (data-model.md §9).
pub const MAX_NOTIFICATION_ACTIONS: usize = 2;

/// Newest-first collection of notifications.
#[derive(Debug, Default)]
pub struct NotificationCenter {
    items: VecDeque<Notification>,
    next_id: u64,
}

impl NotificationCenter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Raise a notification with no arguments. Returns its id.
    pub fn raise(&mut self, severity: Severity, message_key: &'static str) -> u64 {
        self.raise_with_args(severity, message_key, Vec::new())
    }

    /// Raise a notification with Fluent placeholder arguments. Returns its id.
    pub fn raise_with_args(
        &mut self,
        severity: Severity,
        message_key: &'static str,
        args: Vec<(&'static str, String)>,
    ) -> u64 {
        self.raise_full(severity, message_key, args, Vec::new())
    }

    /// Raise a notification with no arguments but an action button (e.g.
    /// `session-expired`'s "Sign in"). Returns its id.
    pub fn raise_with_action(
        &mut self,
        severity: Severity,
        message_key: &'static str,
        action: NotificationAction,
    ) -> u64 {
        self.raise_full(severity, message_key, Vec::new(), vec![action])
    }

    /// Raise a notification with both Fluent placeholder arguments and an
    /// action button. Returns its id.
    pub fn raise_with_args_and_action(
        &mut self,
        severity: Severity,
        message_key: &'static str,
        args: Vec<(&'static str, String)>,
        action: NotificationAction,
    ) -> u64 {
        self.raise_full(severity, message_key, args, vec![action])
    }

    /// Raise a notification with up to `MAX_NOTIFICATION_ACTIONS` action
    /// buttons (data-model.md §9, e.g. `stream-source-unavailable`'s
    /// "Open status page" + "Retry"). Extra actions beyond the cap are
    /// dropped. Returns its id.
    pub fn raise_with_actions(
        &mut self,
        severity: Severity,
        message_key: &'static str,
        args: Vec<(&'static str, String)>,
        mut actions: Vec<NotificationAction>,
    ) -> u64 {
        actions.truncate(MAX_NOTIFICATION_ACTIONS);
        self.raise_full(severity, message_key, args, actions)
    }

    fn raise_full(
        &mut self,
        severity: Severity,
        message_key: &'static str,
        args: Vec<(&'static str, String)>,
        actions: Vec<NotificationAction>,
    ) -> u64 {
        self.raise_full_keyed(severity, message_key, args, actions, None, None)
    }

    fn raise_full_keyed(
        &mut self,
        severity: Severity,
        message_key: &'static str,
        args: Vec<(&'static str, String)>,
        actions: Vec<NotificationAction>,
        dedupe_key: Option<String>,
        attribution: Option<PluginAttribution>,
    ) -> u64 {
        self.raise_full_detailed(
            severity,
            message_key,
            args,
            actions,
            dedupe_key,
            attribution,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn raise_full_detailed(
        &mut self,
        severity: Severity,
        message_key: &'static str,
        args: Vec<(&'static str, String)>,
        actions: Vec<NotificationAction>,
        dedupe_key: Option<String>,
        attribution: Option<PluginAttribution>,
        detail: Option<String>,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let created_at = Instant::now();
        self.items.push_front(Notification {
            id,
            severity,
            message_key,
            args,
            action: actions.first().copied(),
            actions,
            created_at,
            dismissed: false,
            dedupe_key,
            attribution,
            detail,
            auto_dismiss_from: created_at,
            held: false,
        });
        id
    }

    /// Raise a notification carrying non-localised technical `detail` text
    /// shown behind a "Details" toggle (019-notification-presentation,
    /// FR-013, contract C2) — e.g. a raw device id or dropped keybinding
    /// ids, never shown in the main message. No actions, no dedupe key, no
    /// attribution. `detail` must be non-empty (debug-asserted); a release
    /// build stores `None` for an empty string rather than panicking.
    /// Returns the new notification's id.
    ///
    /// # Examples
    ///
    /// ```
    /// use modplayer_core::{NotificationCenter, Severity};
    ///
    /// let mut center = NotificationCenter::new();
    /// let id = center.raise_with_detail(
    ///     Severity::Warning,
    ///     "device-missing-at-launch",
    ///     vec![("device", "Scarlett 2i2".to_string())],
    ///     "coreaudio:device-42".to_string(),
    /// );
    /// let notification = center.visible().find(|n| n.id == id).unwrap();
    /// assert_eq!(notification.detail.as_deref(), Some("coreaudio:device-42"));
    /// ```
    pub fn raise_with_detail(
        &mut self,
        severity: Severity,
        message_key: &'static str,
        args: Vec<(&'static str, String)>,
        detail: String,
    ) -> u64 {
        debug_assert!(
            !detail.is_empty(),
            "raise_with_detail: detail must not be empty"
        );
        let detail = (!detail.is_empty()).then_some(detail);
        self.raise_full_detailed(severity, message_key, args, Vec::new(), None, None, detail)
    }

    /// Test/fixture-only combination of [`Self::raise_with_actions`] and
    /// [`Self::raise_with_detail`] (019-notification-presentation, research
    /// R2): no production raise site needs actions and `detail` together
    /// (contract C4), but `tests/notification_stack.rs`'s SC-001 worst-case
    /// fixture needs a single card exercising every optional row — actions,
    /// "Show more" and "Details" — at once, to bound the tallest a real
    /// card can ever get. Up to `MAX_NOTIFICATION_ACTIONS` actions kept, as
    /// in `raise_with_actions`. `detail` must be non-empty (debug-asserted;
    /// release stores `None` for empty). Returns the new notification's id.
    pub fn raise_with_actions_and_detail(
        &mut self,
        severity: Severity,
        message_key: &'static str,
        args: Vec<(&'static str, String)>,
        mut actions: Vec<NotificationAction>,
        detail: String,
    ) -> u64 {
        actions.truncate(MAX_NOTIFICATION_ACTIONS);
        debug_assert!(
            !detail.is_empty(),
            "raise_with_actions_and_detail: detail must not be empty"
        );
        let detail = (!detail.is_empty()).then_some(detail);
        self.raise_full_detailed(severity, message_key, args, actions, None, None, detail)
    }

    /// Raise a plugin-attributed notification (US5, contracts/overlays-
    /// settings-notify.md §3 N2): an ordinary, non-blocking entry under
    /// this type's own lifecycle (`Info` auto-dismisses, `Warning`/
    /// `Critical` persist) — never a dedupe key, never an action button,
    /// never a modal. Returns the new notification's id.
    pub fn raise_attributed(
        &mut self,
        severity: Severity,
        message_key: &'static str,
        args: Vec<(&'static str, String)>,
        attribution: PluginAttribution,
    ) -> u64 {
        self.raise_full_keyed(
            severity,
            message_key,
            args,
            Vec::new(),
            None,
            Some(attribution),
        )
    }

    /// Raise a notification identified by `dedupe_key` (009 L6): any
    /// existing *visible* notification with the same key is dismissed
    /// first, so a repeat (e.g. a second `plugin-suspended` for the same
    /// plugin) replaces rather than stacks. Returns the new
    /// notification's id.
    pub fn raise_keyed(
        &mut self,
        severity: Severity,
        message_key: &'static str,
        args: Vec<(&'static str, String)>,
        actions: Vec<NotificationAction>,
        dedupe_key: impl Into<String>,
    ) -> u64 {
        let dedupe_key = dedupe_key.into();
        self.dismiss_by_dedupe(&dedupe_key);
        self.raise_full_keyed(severity, message_key, args, actions, Some(dedupe_key), None)
    }

    /// Dismiss every visible notification whose `dedupe_key` matches
    /// (009 L4's "dismiss `plugin-suspended:<identifier>` on `Ready`", and
    /// `raise_keyed`'s own replace-not-stack rule). A no-op when none
    /// match.
    pub fn dismiss_by_dedupe(&mut self, dedupe_key: &str) {
        for item in self.items.iter_mut().filter(|n| !n.dismissed) {
            if item.dedupe_key.as_deref() == Some(dedupe_key) {
                item.dismissed = true;
            }
        }
    }

    /// Age out `Info` notifications whose auto-dismiss timer
    /// (`auto_dismiss_from`) has run past `INFO_AUTO_DISMISS` and that are
    /// not currently held (019-notification-presentation, FR-009, contract
    /// C3 rule H1). `Warning`/`Critical` are never auto-dismissed (H4).
    pub fn tick(&mut self, now: Instant) {
        for item in &mut self.items {
            if item.severity == Severity::Info
                && !item.dismissed
                && !item.held
                && now.saturating_duration_since(item.auto_dismiss_from) >= INFO_AUTO_DISMISS
            {
                item.dismissed = true;
            }
        }
    }

    /// Hold an `Info` notification's auto-dismiss timer while its "Show
    /// more" or "Details" is expanded (019, FR-009, contract C3 rule H2,
    /// WCAG 2.2.1). Idempotent; a no-op for an unknown or already-dismissed
    /// id.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::{Duration, Instant};
    /// use modplayer_core::{NotificationCenter, Severity};
    ///
    /// let mut center = NotificationCenter::new();
    /// let id = center.raise(Severity::Info, "device-available-again");
    /// center.hold_auto_dismiss(id);
    /// center.tick(Instant::now() + Duration::from_secs(3600));
    /// assert!(center.visible().any(|n| n.id == id), "held notifications never age out");
    /// ```
    pub fn hold_auto_dismiss(&mut self, id: u64) {
        if let Some(item) = self.items.iter_mut().find(|n| n.id == id && !n.dismissed) {
            item.held = true;
        }
    }

    /// Release a previously held `Info` notification's auto-dismiss timer,
    /// restarting the `INFO_AUTO_DISMISS` window from `now` (019, contract
    /// C3 rule H3). A no-op if the id is unknown, dismissed, or not
    /// currently held.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Instant;
    /// use modplayer_core::{NotificationCenter, Severity};
    ///
    /// let mut center = NotificationCenter::new();
    /// let id = center.raise(Severity::Info, "device-available-again");
    /// center.hold_auto_dismiss(id);
    /// center.release_auto_dismiss(id, Instant::now());
    /// assert!(center.visible().any(|n| n.id == id));
    /// ```
    pub fn release_auto_dismiss(&mut self, id: u64, now: Instant) {
        if let Some(item) = self
            .items
            .iter_mut()
            .find(|n| n.id == id && !n.dismissed && n.held)
        {
            item.held = false;
            item.auto_dismiss_from = now;
        }
    }

    /// Manually dismiss a notification by id (no-op if unknown or already dismissed).
    pub fn dismiss(&mut self, id: u64) {
        if let Some(item) = self.items.iter_mut().find(|n| n.id == id) {
            item.dismissed = true;
        }
    }

    /// Dismiss every visible notification with the given `message_key`
    /// (e.g. `RefreshRecovered` dismissing the `signin-again` warning
    /// `RefreshFailing` raised — contracts/account-session.md "Events").
    /// A no-op when none match.
    pub fn dismiss_by_key(&mut self, message_key: &str) {
        for item in self.items.iter_mut().filter(|n| !n.dismissed) {
            if item.message_key == message_key {
                item.dismissed = true;
            }
        }
    }

    /// Visible (non-dismissed) notifications, newest first.
    pub fn visible(&self) -> impl Iterator<Item = &Notification> {
        self.items.iter().filter(|n| !n.dismissed)
    }

    /// Every notification ever raised this session, newest first, dismissed or not.
    pub fn all(&self) -> impl Iterator<Item = &Notification> {
        self.items.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raised_notifications_are_newest_first() {
        let mut center = NotificationCenter::new();
        center.raise(Severity::Info, "first");
        center.raise(Severity::Warning, "second");
        let keys: Vec<_> = center.all().map(|n| n.message_key).collect();
        assert_eq!(keys, vec!["second", "first"]);
    }

    #[test]
    fn info_auto_dismisses_after_ten_seconds() {
        let mut center = NotificationCenter::new();
        let id = center.raise(Severity::Info, "info-key");
        let created_at = Instant::now();

        center.tick(created_at + Duration::from_secs(5));
        assert!(
            center.visible().any(|n| n.id == id),
            "still visible before 10s"
        );

        center.tick(created_at + Duration::from_secs(11));
        assert!(
            !center.visible().any(|n| n.id == id),
            "auto-dismissed after 10s"
        );
    }

    #[test]
    fn warning_and_critical_persist_until_dismissed() {
        let mut center = NotificationCenter::new();
        let warning_id = center.raise(Severity::Warning, "warning-key");
        let critical_id = center.raise(Severity::Critical, "critical-key");

        center.tick(Instant::now() + Duration::from_secs(3600));
        assert!(center.visible().any(|n| n.id == warning_id));
        assert!(center.visible().any(|n| n.id == critical_id));

        center.dismiss(warning_id);
        center.dismiss(critical_id);
        assert!(!center.visible().any(|n| n.id == warning_id));
        assert!(!center.visible().any(|n| n.id == critical_id));
    }

    #[test]
    fn dismiss_is_a_no_op_for_unknown_id() {
        let mut center = NotificationCenter::new();
        center.raise(Severity::Info, "key");
        center.dismiss(9999);
        assert_eq!(center.visible().count(), 1);
    }

    #[test]
    fn plain_raise_has_no_action() {
        let mut center = NotificationCenter::new();
        center.raise(Severity::Info, "key");
        assert_eq!(center.visible().next().and_then(|n| n.action), None);
    }

    #[test]
    fn raise_with_action_attaches_the_action() {
        let mut center = NotificationCenter::new();
        center.raise_with_action(
            Severity::Critical,
            "session-expired",
            NotificationAction::SignIn,
        );
        assert_eq!(
            center.visible().next().and_then(|n| n.action),
            Some(NotificationAction::SignIn)
        );
    }

    #[test]
    fn dismiss_by_key_dismisses_every_matching_visible_notification() {
        let mut center = NotificationCenter::new();
        center.raise(Severity::Warning, "signin-again");
        center.raise(Severity::Info, "unrelated");
        center.raise(Severity::Warning, "signin-again");

        center.dismiss_by_key("signin-again");

        let remaining: Vec<_> = center.visible().map(|n| n.message_key).collect();
        assert_eq!(remaining, vec!["unrelated"]);
    }

    #[test]
    fn dismiss_by_key_is_a_no_op_when_nothing_matches() {
        let mut center = NotificationCenter::new();
        center.raise(Severity::Info, "key");
        center.dismiss_by_key("no-such-key");
        assert_eq!(center.visible().count(), 1);
    }

    // -------------------------------------------------------------------
    // 019-notification-presentation, Phase 6/US4 (T018): `detail`,
    // `hold_auto_dismiss`/`release_auto_dismiss`, and the refined `tick`
    // rules H1-H5 (contract C1-C3, data-model.md §1).
    // -------------------------------------------------------------------

    /// Every existing `raise*` method sets `detail: None` (contract C1) —
    /// only `raise_with_detail` ever populates it.
    #[test]
    fn existing_raise_methods_leave_detail_none() {
        let mut center = NotificationCenter::new();
        center.raise(Severity::Info, "key");
        center.raise_with_args(Severity::Info, "key", vec![("a", "b".to_string())]);
        center.raise_with_action(Severity::Warning, "key", NotificationAction::SignIn);
        center.raise_keyed(Severity::Warning, "key", Vec::new(), Vec::new(), "dedupe");
        for n in center.all() {
            assert_eq!(n.detail, None);
        }
    }

    /// `raise_with_detail` stores the given text and no actions/dedupe/
    /// attribution.
    #[test]
    fn raise_with_detail_stores_the_detail_text() {
        let mut center = NotificationCenter::new();
        let id = center.raise_with_detail(
            Severity::Warning,
            "device-missing-at-launch",
            vec![("device", "Scarlett 2i2".to_string())],
            "coreaudio:device-42".to_string(),
        );
        let notification = center
            .visible()
            .find(|n| n.id == id)
            .unwrap_or_else(|| unreachable!());
        assert_eq!(notification.detail.as_deref(), Some("coreaudio:device-42"));
        assert!(notification.actions.is_empty());
        assert_eq!(notification.dedupe_key, None);
        assert!(notification.attribution.is_none());
    }

    /// H1: a held `Info` notification never ages out no matter how much
    /// time passes.
    #[test]
    fn held_info_never_auto_dismisses() {
        let mut center = NotificationCenter::new();
        let id = center.raise(Severity::Info, "key");
        center.hold_auto_dismiss(id);
        center.tick(Instant::now() + Duration::from_secs(3600));
        assert!(center.visible().any(|n| n.id == id));
    }

    /// H2: `hold_auto_dismiss` is idempotent and a no-op for an unknown or
    /// already-dismissed id.
    #[test]
    fn hold_auto_dismiss_is_idempotent_and_noop_for_unknown_or_dismissed() {
        let mut center = NotificationCenter::new();
        let id = center.raise(Severity::Info, "key");
        center.hold_auto_dismiss(id);
        center.hold_auto_dismiss(id); // idempotent
        center.tick(Instant::now() + Duration::from_secs(3600));
        assert!(center.visible().any(|n| n.id == id));

        center.hold_auto_dismiss(999_999); // unknown id: no panic, no effect

        let dismissed_id = center.raise(Severity::Info, "other");
        center.dismiss(dismissed_id);
        center.hold_auto_dismiss(dismissed_id); // no-op: already dismissed
        assert!(!center.visible().any(|n| n.id == dismissed_id));
    }

    /// H3: `release_auto_dismiss` restarts the 10s window from `now`
    /// rather than the original raise time.
    #[test]
    fn release_auto_dismiss_restarts_the_window_from_now() {
        let mut center = NotificationCenter::new();
        let id = center.raise(Severity::Info, "key");
        let raised_at = Instant::now();

        center.hold_auto_dismiss(id);
        // Held well past the original 10s window: still visible.
        center.tick(raised_at + Duration::from_secs(20));
        assert!(center.visible().any(|n| n.id == id));

        let release_at = raised_at + Duration::from_secs(20);
        center.release_auto_dismiss(id, release_at);
        // 9s after release: still visible (a fresh window, not the stale one).
        center.tick(release_at + Duration::from_secs(9));
        assert!(center.visible().any(|n| n.id == id));
        // 10s after release: dismissed.
        center.tick(release_at + Duration::from_secs(10));
        assert!(!center.visible().any(|n| n.id == id));
    }

    /// H3 (no-op half): releasing a notification that was never held does
    /// nothing (its window is untouched).
    #[test]
    fn release_auto_dismiss_is_a_no_op_when_not_held() {
        let mut center = NotificationCenter::new();
        let id = center.raise(Severity::Info, "key");
        let raised_at = Instant::now();
        // Not held; releasing must not reset the window.
        center.release_auto_dismiss(id, raised_at + Duration::from_secs(5));
        center.tick(raised_at + Duration::from_secs(11));
        assert!(
            !center.visible().any(|n| n.id == id),
            "release on a non-held notification must not extend its window"
        );
    }

    /// H4: `Warning`/`Critical` are unaffected by hold/release (they never
    /// auto-dismiss regardless).
    #[test]
    fn warning_and_critical_ignore_hold_and_release() {
        let mut center = NotificationCenter::new();
        let warning_id = center.raise(Severity::Warning, "key");
        let critical_id = center.raise(Severity::Critical, "key");
        center.hold_auto_dismiss(warning_id);
        center.hold_auto_dismiss(critical_id);
        center.tick(Instant::now() + Duration::from_secs(3600));
        assert!(center.visible().any(|n| n.id == warning_id));
        assert!(center.visible().any(|n| n.id == critical_id));
    }

    /// H5: an explicit `dismiss` still dismisses a held notification.
    #[test]
    fn explicit_dismiss_still_dismisses_a_held_notification() {
        let mut center = NotificationCenter::new();
        let id = center.raise(Severity::Info, "key");
        center.hold_auto_dismiss(id);
        center.dismiss(id);
        assert!(!center.visible().any(|n| n.id == id));
    }
}

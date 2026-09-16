// SPDX-License-Identifier: MIT OR Apache-2.0

//! Notifications: severity-classified, newest-first, Fluent-keyed messages
//! (data-model.md §5.3, §6.4; FR-016). `Info` auto-dismisses at 10 s;
//! `Warning`/`Critical` persist until manually dismissed.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

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

/// Notification severity (data-model.md §6.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Critical,
    Warning,
    Info,
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
        let id = self.next_id;
        self.next_id += 1;
        self.items.push_front(Notification {
            id,
            severity,
            message_key,
            args,
            action: actions.first().copied(),
            actions,
            created_at: Instant::now(),
            dismissed: false,
        });
        id
    }

    /// Age out `Info` notifications older than `INFO_AUTO_DISMISS`.
    /// `Warning`/`Critical` are never auto-dismissed.
    pub fn tick(&mut self, now: Instant) {
        for item in &mut self.items {
            if item.severity == Severity::Info
                && !item.dismissed
                && now.saturating_duration_since(item.created_at) >= INFO_AUTO_DISMISS
            {
                item.dismissed = true;
            }
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
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! The notification area (contracts/ui-surface.md "Main window"): a
//! newest-first stack, each item a severity icon, severity text, message,
//! and Dismiss button. `Info` items auto-dismiss via
//! `NotificationCenter::tick` (called once per frame by the caller); this
//! widget never wraps itself in a modal window and never disables any
//! other widget — the caller (`app.rs`) draws it as a floating, non-
//! blocking overlay so it never interrupts playback or navigation.
//!
//! Colour never carries meaning alone (Constitution/FR-022): each item's
//! severity is also spelled out as text, not just an icon/colour.

use egui::{Frame, Ui};
use modplayer_core::{Notification, NotificationAction, NotificationCenter, Severity, tr, tr_args};

/// What the user did with the notification stack this frame, if anything
/// (002-first-launch-and-sign-in contracts/ui-surface.md "notifications
/// with actions").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NotificationInteraction {
    /// The notification the user clicked Dismiss on, if any — the caller
    /// applies it via `NotificationCenter::dismiss`.
    pub dismissed: Option<u64>,
    /// The notification whose action button the user clicked, and which
    /// action it was (e.g. `session-expired`'s "Sign in") — the caller
    /// reacts to it (starting US2 T071; this phase only renders the
    /// button).
    pub action_clicked: Option<(u64, NotificationAction)>,
}

/// Draw every visible (non-dismissed) notification, newest first: a
/// severity icon+text, the message, an optional action button
/// (`notification.action`), and Dismiss.
///
/// Each item sits in an opaque popup-style frame: the caller floats this
/// stack over the current screen, so without a background the text would
/// collide with whatever is underneath (found by quickstart M4.2).
pub fn show(ui: &mut Ui, center: &NotificationCenter) -> NotificationInteraction {
    let mut interaction = NotificationInteraction::default();
    for notification in center.visible() {
        Frame::popup(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(severity_label(notification.severity));
                ui.label(message(notification));
                if let Some(action) = notification.action
                    && ui.button(tr(action_label_key(action))).clicked()
                {
                    interaction.action_clicked = Some((notification.id, action));
                }
                if ui.button(tr("notification-dismiss")).clicked() {
                    interaction.dismissed = Some(notification.id);
                }
            });
        });
    }
    interaction
}

/// The Fluent key for an action's button label.
fn action_label_key(action: NotificationAction) -> &'static str {
    match action {
        NotificationAction::SignIn => "notification-action-sign-in",
    }
}

/// Severity icon (decorative) + the severity spelled out as text
/// (`severity-critical` / `-warning` / `-info`) — the accessible name for
/// this label is the whole string, so the icon glyph never carries meaning
/// on its own.
fn severity_label(severity: Severity) -> String {
    let (glyph, key) = match severity {
        Severity::Critical => ("⛔", "severity-critical"),
        Severity::Warning => ("⚠", "severity-warning"),
        Severity::Info => ("ℹ", "severity-info"),
    };
    format!("{glyph} {}", tr(key))
}

/// Resolve a notification's message, substituting its Fluent `args` when it
/// has any (e.g. `device-lost`'s `{ $device }`).
fn message(notification: &Notification) -> String {
    if notification.args.is_empty() {
        tr(notification.message_key)
    } else {
        tr_args(notification.message_key, &notification.args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_label_includes_the_severity_text() {
        assert!(severity_label(Severity::Critical).contains(&tr("severity-critical")));
        assert!(severity_label(Severity::Warning).contains(&tr("severity-warning")));
        assert!(severity_label(Severity::Info).contains(&tr("severity-info")));
    }

    #[test]
    fn message_falls_back_to_the_plain_key_with_no_args() {
        let notification = Notification {
            id: 0,
            severity: Severity::Info,
            message_key: "no-output-devices",
            args: Vec::new(),
            action: None,
            created_at: std::time::Instant::now(),
            dismissed: false,
        };
        assert_eq!(message(&notification), tr("no-output-devices"));
    }

    #[test]
    fn action_label_key_resolves_a_fluent_key_for_every_action() {
        assert_eq!(
            action_label_key(NotificationAction::SignIn),
            "notification-action-sign-in"
        );
    }

    #[test]
    fn default_interaction_is_empty() {
        let interaction = NotificationInteraction::default();
        assert_eq!(interaction.dismissed, None);
        assert_eq!(interaction.action_clicked, None);
    }
}

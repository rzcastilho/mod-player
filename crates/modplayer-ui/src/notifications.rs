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
use modplayer_core::{Notification, NotificationCenter, Severity, tr, tr_args};

/// Draw every visible (non-dismissed) notification, newest first. Returns
/// the id of the notification the user clicked Dismiss on this frame, if
/// any — the caller applies it via `NotificationCenter::dismiss`.
///
/// Each item sits in an opaque popup-style frame: the caller floats this
/// stack over the current screen, so without a background the text would
/// collide with whatever is underneath (found by quickstart M4.2).
pub fn show(ui: &mut Ui, center: &NotificationCenter) -> Option<u64> {
    let mut dismissed = None;
    for notification in center.visible() {
        Frame::popup(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(severity_label(notification.severity));
                ui.label(message(notification));
                if ui.button(tr("notification-dismiss")).clicked() {
                    dismissed = Some(notification.id);
                }
            });
        });
    }
    dismissed
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
            created_at: std::time::Instant::now(),
            dismissed: false,
        };
        assert_eq!(message(&notification), tr("no-output-devices"));
    }
}

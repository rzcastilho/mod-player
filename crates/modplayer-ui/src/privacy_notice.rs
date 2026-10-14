// SPDX-License-Identifier: MIT OR Apache-2.0

//! Privacy Notice screen (contracts/ui-surface.md "Privacy Notice"):
//! scrollable, readable-offline text reachable from Welcome (US1) and,
//! later, Settings › About (US2) — identical content from both entry
//! points, so this module is stateless and takes no screen-specific
//! arguments.

use egui::{ScrollArea, Ui};
use modplayer_core::tr;

use crate::theme;

/// Which screen opened the privacy notice, so the back control can name
/// where it returns to (030 C5).
///
/// ```
/// use modplayer_ui::privacy_notice::PrivacyOrigin;
/// let origin = PrivacyOrigin::About;
/// assert_ne!(origin, PrivacyOrigin::Welcome);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivacyOrigin {
    /// Opened from the first-launch welcome screen.
    Welcome,
    /// Opened from Settings › About.
    About,
}

/// Draw the privacy notice. Returns `true` once the back control is
/// clicked, so the caller (`welcome.rs`, `settings/about.rs`) can pop back
/// to whichever screen opened this one. The control is labelled for
/// `origin` (`privacy-back-to-welcome` / `privacy-back-to-about`).
///
/// ```no_run
/// # use modplayer_ui::privacy_notice::{self, PrivacyOrigin};
/// # fn demo(ui: &mut egui::Ui) {
/// if privacy_notice::show(ui, PrivacyOrigin::About) {
///     // Return to Settings › About.
/// }
/// # }
/// ```
pub fn show(ui: &mut Ui, origin: PrivacyOrigin) -> bool {
    ui.heading(tr("privacy-title"));
    ScrollArea::vertical().show(ui, |ui| {
        // FR-006, U2: the privacy notice is prose, capped at the
        // 72-character measure rather than spanning the window.
        ui.set_max_width(ui.available_width().min(theme::body_measure(ui.ctx())));
        ui.label(tr("privacy-body"));
    });
    let back_key = match origin {
        PrivacyOrigin::Welcome => "privacy-back-to-welcome",
        PrivacyOrigin::About => "privacy-back-to-about",
    };
    ui.button(tr(back_key)).clicked()
}

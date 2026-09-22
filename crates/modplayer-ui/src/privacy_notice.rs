// SPDX-License-Identifier: MIT OR Apache-2.0

//! Privacy Notice screen (contracts/ui-surface.md "Privacy Notice"):
//! scrollable, readable-offline text reachable from Welcome (US1) and,
//! later, Settings › About (US2) — identical content from both entry
//! points, so this module is stateless and takes no screen-specific
//! arguments.

use egui::{ScrollArea, Ui};
use modplayer_core::tr;

use crate::theme;

/// Draw the privacy notice. Returns `true` once "Back" is clicked, so the
/// caller (`welcome.rs`, and later `settings/about.rs`) can pop back to
/// whichever screen opened this one.
pub fn show(ui: &mut Ui) -> bool {
    ui.heading(tr("privacy-title"));
    ScrollArea::vertical().show(ui, |ui| {
        // FR-006, U2: the privacy notice is prose, capped at the
        // 72-character measure rather than spanning the window.
        ui.set_max_width(ui.available_width().min(theme::body_measure(ui.ctx())));
        ui.label(tr("privacy-body"));
    });
    ui.button(tr("privacy-back")).clicked()
}

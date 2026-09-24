// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Language settings screen (contracts/ui-surface.md's
//! `language.locale` descriptor, T093): a locale combo with English as the
//! only option this slice (FR-021 — pt-BR ships in a later slice).

use egui::{ComboBox, Ui};
use modplayer_core::tr;

use crate::theme;

/// Draw the Language settings screen. `focus` is
/// `Some("language.locale")` the frame a settings-search result asks to
/// land here (`settings/mod.rs`'s `SettingsScreen`, T090).
pub fn show(ui: &mut Ui, focus: Option<&str>) {
    ui.label(tr("setting-locale"));
    // FR-006, U2: field-description prose, capped at the 72-character
    // measure (research R17).
    ui.scope(|ui| {
        ui.set_max_width(ui.available_width().min(theme::body_measure(ui.ctx())));
        ui.label(tr("setting-locale-desc"));
    });

    let response = ComboBox::from_id_salt("language.locale")
        .selected_text(tr("language-english"))
        .show_ui(ui, |ui| {
            let _ = ui.selectable_label(true, tr("language-english"));
        })
        .response;
    if focus == Some("language.locale") {
        response.request_focus();
    }
}

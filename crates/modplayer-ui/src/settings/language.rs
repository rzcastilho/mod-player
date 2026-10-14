// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Language settings screen (contracts/ui-surface.md's
//! `language.locale` descriptor, T093): a locale combo with English as the
//! only option this slice (FR-021 — pt-BR ships in a later slice).

use egui::{ComboBox, Ui};
use modplayer_core::tr;

use crate::settings::field::{self, FieldSpec};
use crate::widgets::controls::panel_card;

/// Draw the Language settings screen. `focus` is
/// `Some("language.locale")` the frame a settings-search result asks to
/// land here (`settings/mod.rs`'s `SettingsScreen`, T090).
pub fn show(ui: &mut Ui, focus: Option<&str>) {
    panel_card(ui, &tr("settings-group-language"), |ui| {
        let mut spec = FieldSpec::new("language.locale", tr("setting-locale"));
        spec.help = Some(tr("setting-locale-desc"));
        let output = field::row(ui, &spec, |ui| {
            ComboBox::from_id_salt("language.locale")
                .selected_text(tr("language-english"))
                .show_ui(ui, |ui| {
                    let _ = ui.selectable_label(true, tr("language-english"));
                })
                .response
        });
        if focus == Some("language.locale") {
            output.control.request_focus();
        }
    });
}

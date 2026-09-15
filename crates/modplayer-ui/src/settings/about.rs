// SPDX-License-Identifier: MIT OR Apache-2.0

//! Settings › About (contracts/ui-surface.md "Settings › About"): product
//! description, version, a link to the Privacy Notice, and a read-only
//! disclosure view (the same `disclosure-*` labels Welcome uses, minus the
//! acknowledge/decline actions — this is a reference view, not a gate).

use egui::Ui;
use modplayer_account::TERMS_URL;
use modplayer_core::{tr, tr_args};

use crate::privacy_notice;

/// Which sub-view Settings › About is currently showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum View {
    #[default]
    Main,
    Privacy,
    Disclosure,
}

/// Local UI state for one About screen instance — `SettingsScreen` owns
/// one for the app's lifetime (mirroring `WelcomeScreen`).
#[derive(Debug, Default)]
pub struct AboutScreen {
    view: View,
}

/// Draw the current sub-view.
pub fn show(ui: &mut Ui, screen: &mut AboutScreen) {
    match screen.view {
        View::Main => show_main(ui, screen),
        View::Privacy => {
            if privacy_notice::show(ui) {
                screen.view = View::Main;
            }
        }
        View::Disclosure => show_disclosure(ui, screen),
    }
}

fn show_main(ui: &mut Ui, screen: &mut AboutScreen) {
    ui.label(tr("about-product"));
    ui.label(tr_args(
        "about-version",
        &[("version", env!("CARGO_PKG_VERSION").to_string())],
    ));
    if ui.button(tr("about-privacy")).clicked() {
        screen.view = View::Privacy;
    }
    if ui.button(tr("about-disclosure")).clicked() {
        screen.view = View::Disclosure;
    }
}

/// The same disclosure text Welcome shows (`welcome-description` doubles
/// as `about-product`, per contract), but read-only: no acknowledge/
/// decline, just a `privacy-back` button back to the About screen.
fn show_disclosure(ui: &mut Ui, screen: &mut AboutScreen) {
    ui.label(tr("welcome-description"));
    ui.label(tr("disclosure-unofficial"));
    ui.label(tr("disclosure-premium-required"));
    ui.label(tr("disclosure-terms-apply"));
    ui.hyperlink_to(tr("disclosure-terms-link"), TERMS_URL);
    if ui.button(tr("privacy-back")).clicked() {
        screen.view = View::Main;
    }
}

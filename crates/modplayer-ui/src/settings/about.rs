// SPDX-License-Identifier: MIT OR Apache-2.0

//! Settings › About (contracts/ui-surface.md "Settings › About"): product
//! description, version, a link to the Privacy Notice, and a read-only
//! disclosure view (the same `disclosure-*` labels Welcome uses, minus the
//! acknowledge/decline actions — this is a reference view, not a gate).

use egui::Ui;
use egui::accesskit::Role;
use modplayer_account::TERMS_URL;
use modplayer_core::{tr, tr_args};

use crate::privacy_notice::{self, PrivacyOrigin};
use crate::settings::field::{self, FieldSpec};
use crate::theme;
use crate::theme::controls::Variant;
use crate::widgets::controls::button;

/// What happened on Settings › About this frame (030, UX-10).
///
/// ```
/// use modplayer_ui::settings::about::AboutOutcome;
/// assert_ne!(AboutOutcome::None, AboutOutcome::RestoreGettingStarted);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AboutOutcome {
    None,
    /// "Show getting-started card" was activated.
    RestoreGettingStarted,
}

/// Descriptor id of the restore row (also its search id).
///
/// ```
/// assert_eq!(
///     modplayer_ui::settings::about::SHOW_GETTING_STARTED_ID,
///     "about.show_getting_started"
/// );
/// ```
pub const SHOW_GETTING_STARTED_ID: &str = "about.show_getting_started";

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

/// Draw the current sub-view. `getting_started_dismissed` is whether the
/// Library's getting-started card is currently hidden.
pub fn show(
    ui: &mut Ui,
    screen: &mut AboutScreen,
    getting_started_dismissed: bool,
) -> AboutOutcome {
    match screen.view {
        View::Main => return show_main(ui, screen, getting_started_dismissed),
        View::Privacy => {
            if privacy_notice::show(ui, PrivacyOrigin::About) {
                screen.view = View::Main;
            }
        }
        View::Disclosure => show_disclosure(ui, screen),
    }
    AboutOutcome::None
}

fn show_main(
    ui: &mut Ui,
    screen: &mut AboutScreen,
    getting_started_dismissed: bool,
) -> AboutOutcome {
    // FR-006, U2: the product description is prose, capped at the
    // 72-character measure (research R17).
    ui.scope(|ui| {
        ui.set_max_width(ui.available_width().min(theme::body_measure(ui.ctx())));
        ui.label(tr("about-product"));
    });
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
    show_restore_row(ui, getting_started_dismissed)
}

/// UX-10: the searchable "Show getting-started card" row. Disabled, with
/// an explanatory accessible description, while the card is visible.
fn show_restore_row(ui: &mut Ui, dismissed: bool) -> AboutOutcome {
    let label = tr("setting-show-getting-started");
    let mut spec = FieldSpec::new(SHOW_GETTING_STARTED_ID, label.clone());
    spec.help = Some(tr("setting-show-getting-started-desc"));
    spec.label_in_control = true;
    let mut outcome = AboutOutcome::None;
    field::row(ui, &spec, |ui| {
        ui.add_enabled_ui(dismissed, |ui| {
            let response = button(ui, Variant::Default, label);
            ui.ctx().accesskit_node_builder(response.id, |b| {
                b.set_role(Role::Button);
                if !dismissed {
                    b.set_description(tr("setting-show-getting-started-visible"));
                }
            });
            if response.clicked() {
                outcome = AboutOutcome::RestoreGettingStarted;
            }
            response
        })
        .inner
    });
    outcome
}

/// The same disclosure text Welcome shows (`welcome-description` doubles
/// as `about-product`, per contract), but read-only: no acknowledge/
/// decline, just a `privacy-back` button back to the About screen.
fn show_disclosure(ui: &mut Ui, screen: &mut AboutScreen) {
    // FR-006, U2: prose, capped at the 72-character measure.
    ui.scope(|ui| {
        ui.set_max_width(ui.available_width().min(theme::body_measure(ui.ctx())));
        ui.label(tr("welcome-description"));
        ui.label(tr("disclosure-unofficial"));
        ui.label(tr("disclosure-premium-required"));
        ui.label(tr("disclosure-terms-apply"));
    });
    ui.hyperlink_to(tr("disclosure-terms-link"), TERMS_URL);
    if ui.button(tr("privacy-back")).clicked() {
        screen.view = View::Main;
    }
}

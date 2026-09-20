// SPDX-License-Identifier: MIT OR Apache-2.0

//! The dismissible "Getting started" card (013-key-and-tempo-plugin,
//! contracts/getting-started-card.md): names both bundled plugins
//! (Section Loop, Key & Tempo) and their default shortcuts, and links to
//! the placeholder plugin tutorial. Host-native and stateless — it holds
//! no controller/session reference of its own and never calls
//! `open_url` itself (design note 11: only `App` does that).

use egui::{Frame, Ui, accesskit::Role};
use modplayer_core::tr;

/// What happened this frame, if anything (contracts/getting-started-
/// card.md C1/C2). `App::show_library` is the only caller, and the only
/// place that acts on `OpenTutorial`/`Dismiss`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GettingStartedOutcome {
    None,
    OpenTutorial,
    Dismiss,
}

/// Draw the card: heading, the two plugin lines, and the tutorial/dismiss
/// buttons (contracts/getting-started-card.md §2). Non-modal — an
/// ordinary framed group inside the caller's scroll area (V4); never
/// blocks input to the rest of the window.
pub fn show(ui: &mut Ui) -> GettingStartedOutcome {
    let mut outcome = GettingStartedOutcome::None;
    Frame::group(ui.style()).show(ui, |ui| {
        ui.heading(tr("getting-started-title"));
        ui.label(tr("getting-started-section-loop"));
        ui.label(tr("getting-started-key-tempo"));
        ui.horizontal(|ui| {
            let tutorial = ui.button(tr("getting-started-tutorial"));
            // X1: the tutorial control is a link, not a plain button.
            ui.ctx().accesskit_node_builder(tutorial.id, |b| {
                b.set_role(Role::Link);
            });
            if tutorial.clicked() {
                outcome = GettingStartedOutcome::OpenTutorial;
            }
            if ui.button(tr("getting-started-dismiss")).clicked() {
                outcome = GettingStartedOutcome::Dismiss;
            }
        });
    });
    outcome
}

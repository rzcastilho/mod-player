// SPDX-License-Identifier: MIT OR Apache-2.0

//! Welcome + Decline views (contracts/ui-surface.md "Welcome"): the
//! first-launch disclosure gate, shown whenever
//! `acknowledged_version != DISCLOSURE_BUNDLE_VERSION` (data-model.md
//! §2.4). Acknowledging records a `DisclosureAcknowledgement` via the
//! existing atomic `settings.toml` save and still proceeds if the save
//! fails, raising `settings-save-failed` (FR-003). Declining writes
//! nothing at all and closes the app (FR-002, SC-006) — the Decline view
//! replaces the same screen's content rather than opening a new one
//! (contracts/ui-surface.md).

use egui::{Ui, ViewportCommand};
use modplayer_account::{DISCLOSURE_BUNDLE_VERSION, TERMS_URL};
use modplayer_audio_io::OutputBackend;
use modplayer_core::{DisclosureAcknowledgement, PlaybackController, Severity, tr};

use crate::privacy_notice;

/// Which sub-view the Welcome screen is currently showing (contracts/
/// ui-surface.md: "Decline view (same screen, replaces content)").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum View {
    Disclosure,
    Decline,
    Privacy,
}

/// Local UI state for one Welcome screen instance: which sub-view is
/// currently showing. `App` owns one instance for the lifetime of the
/// launch gate (mirroring `DeviceCheckScreen`).
pub struct WelcomeScreen {
    view: View,
}

impl Default for WelcomeScreen {
    fn default() -> Self {
        Self {
            view: View::Disclosure,
        }
    }
}

/// What happened this frame, for the caller (`app.rs`) to react to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WelcomeOutcome {
    /// The acknowledgement was recorded (data-model.md §1.1, DM-27); the
    /// caller must update its own cached `disclosure_acknowledged_version`
    /// so the launch gate advances this same frame (data-model.md §2.4 is
    /// evaluated every frame, but only from state `App` holds itself).
    Acknowledged,
}

impl WelcomeScreen {
    /// Draw the current sub-view. Returns `Some(Acknowledged)` the frame
    /// the acknowledge action is recorded; `None` otherwise (including
    /// every frame of the Decline/Privacy sub-views).
    pub fn show<B: OutputBackend>(
        &mut self,
        ui: &mut Ui,
        controller: &mut PlaybackController<B>,
    ) -> Option<WelcomeOutcome> {
        match self.view {
            View::Disclosure => self.show_disclosure(ui, controller),
            View::Decline => {
                show_decline(ui);
                None
            }
            View::Privacy => {
                if privacy_notice::show(ui) {
                    self.view = View::Disclosure;
                }
                None
            }
        }
    }

    fn show_disclosure<B: OutputBackend>(
        &mut self,
        ui: &mut Ui,
        controller: &mut PlaybackController<B>,
    ) -> Option<WelcomeOutcome> {
        ui.heading(tr("welcome-title"));
        ui.label(tr("welcome-description"));
        ui.label(tr("disclosure-unofficial"));
        ui.label(tr("disclosure-premium-required"));
        ui.label(tr("disclosure-terms-apply"));
        ui.hyperlink_to(tr("disclosure-terms-link"), TERMS_URL);
        if ui.button(tr("disclosure-privacy-link")).clicked() {
            self.view = View::Privacy;
        }

        let mut outcome = None;
        ui.horizontal(|ui| {
            // Enabled immediately, default focus (contracts/ui-surface.md)
            // — drawn first so it is first in egui's creation-order focus
            // chain, matching the nav rail's convention (shell.rs).
            if ui.button(tr("welcome-acknowledge")).clicked() {
                acknowledge(controller);
                outcome = Some(WelcomeOutcome::Acknowledged);
            }
            if ui.button(tr("welcome-decline")).clicked() {
                self.view = View::Decline;
            }
        });
        outcome
    }
}

fn show_decline(ui: &mut Ui) {
    ui.label(tr("decline-explanation"));
    if ui.button(tr("decline-quit")).clicked() {
        handle_decline(ui.ctx());
    }
}

/// The Decline action (FR-002, SC-006): close the app. Never touches
/// `settings_store`, the account state store, or the secure store —
/// declining requires no undo because nothing was ever written. Exposed
/// standalone (rather than inlined in `show_decline`) so it is the exact
/// function under test in `decline_writes_nothing`.
pub fn handle_decline(ctx: &egui::Context) {
    ctx.send_viewport_cmd(ViewportCommand::Close);
}

/// The Acknowledge action (FR-003, DM-27): record a
/// `DisclosureAcknowledgement` for the current bundle version via the
/// existing atomic `settings.toml` save (`PlaybackController::confirm_device`'s
/// same load-mutate-save pattern), raising `settings-save-failed` and still
/// proceeding — an unsaved acknowledgement re-shows Welcome next launch,
/// which is safe, but must never block this launch.
fn acknowledge<B: OutputBackend>(controller: &mut PlaybackController<B>) {
    let mut settings = controller.settings_store().load().settings;
    settings.disclosure = Some(DisclosureAcknowledgement::now(DISCLOSURE_BUNDLE_VERSION));
    if controller.settings_store().save(&settings).is_err() {
        controller
            .notifications_mut()
            .raise(Severity::Warning, "settings-save-failed");
    }
}

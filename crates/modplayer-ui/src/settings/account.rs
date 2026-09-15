// SPDX-License-Identifier: MIT OR Apache-2.0

//! Settings › Account (contracts/ui-surface.md "Settings › Account"): the
//! signed-in view (display name, tier, last validated, Re-check
//! subscription — T067), the sign-out confirmation modal (T085), and the
//! signed-out view (T087). Returns any `AccountEvent`s a command (Sign in,
//! Sign out) raised this frame so `App::handle_account_event` (US2 T071 /
//! US3 T086) can map them to notifications/screens exactly like the
//! sign-in step's own commands do.

use egui::{Id, Modal, RichText, Ui};
use modplayer_account::{AccountEvent, AccountService, SessionState, Tier};
use modplayer_core::{tr, tr_args};

/// Draw the Settings › Account screen for the current session state.
pub fn show(ui: &mut Ui, account: &mut AccountService) -> Vec<AccountEvent> {
    match account.state() {
        SessionState::Active | SessionState::Expired => show_signed_in(ui, account),
        _ => show_signed_out(ui, account),
    }
}

fn show_signed_in(ui: &mut Ui, account: &mut AccountService) -> Vec<AccountEvent> {
    let Some(session) = account.account() else {
        ui.label(tr("placeholder-settings-category"));
        return Vec::new();
    };

    ui.label(tr_args(
        "account-display-name",
        &[("name", session.display_name.clone())],
    ));
    ui.label(format!(
        "{}: {}",
        tr("account-tier"),
        tr(tier_key(session.tier))
    ));
    match session.last_validated_at {
        Some(when) => ui.label(tr_args(
            "account-last-validated",
            &[("when", format!("{when}"))],
        )),
        None => ui.label(tr("account-never-validated")),
    };

    ui.label(tr("account-recheck-desc"));
    if ui.button(tr("account-recheck")).clicked() {
        account.recheck_tier();
    }

    ui.separator();

    if ui.button(tr("account-sign-out")).clicked() {
        open_sign_out_modal(ui);
    }

    show_sign_out_modal(ui, account)
}

/// The signed-out Settings › Account view (T087, FR-012, FR-022):
/// unreachable in ordinary use today — a signed-out session shows the
/// sign-in step for the whole window, never Main (ui-surface.md) — except
/// for the one frame `sign_out()` itself completes on, and any other frame
/// reached before the launch gate re-evaluates.
fn show_signed_out(ui: &mut Ui, account: &mut AccountService) -> Vec<AccountEvent> {
    ui.label(tr("account-signed-out"));
    if ui.button(tr("account-sign-in")).clicked() {
        return account.start_sign_in();
    }
    Vec::new()
}

/// Per-`egui::Memory` id: is the sign-out confirmation modal open.
fn modal_open_id() -> Id {
    Id::new("settings-account-signout-modal-open")
}

/// Per-`egui::Memory` id: does the modal shown this frame need to move
/// keyboard focus to Cancel (set the frame it opens, cleared once done —
/// so focus lands on Cancel once, without re-stealing it from the user
/// every subsequent frame while the modal stays open).
fn modal_focus_pending_id() -> Id {
    Id::new("settings-account-signout-modal-focus-pending")
}

fn open_sign_out_modal(ui: &mut Ui) {
    ui.memory_mut(|memory| {
        memory.data.insert_temp(modal_open_id(), true);
        memory.data.insert_temp(modal_focus_pending_id(), true);
    });
}

fn close_sign_out_modal(ui: &mut Ui) {
    ui.memory_mut(|memory| memory.data.insert_temp(modal_open_id(), false));
}

/// The sign-out confirmation modal (contracts/ui-surface.md "Settings ›
/// Account", T085): one bullet per `signout_categories()` category, a
/// destructive "Sign out" and a default-focus "Cancel". Dismissing via
/// Escape or a backdrop click (`ModalResponse::should_close`) behaves like
/// Cancel — sign-out only ever happens on an explicit confirm click.
fn show_sign_out_modal(ui: &mut Ui, account: &mut AccountService) -> Vec<AccountEvent> {
    let is_open = ui.memory(|memory| {
        memory
            .data
            .get_temp::<bool>(modal_open_id())
            .unwrap_or(false)
    });
    if !is_open {
        return Vec::new();
    }
    let mut focus_pending = ui.memory(|memory| {
        memory
            .data
            .get_temp::<bool>(modal_focus_pending_id())
            .unwrap_or(false)
    });

    let categories = account.signout_categories();
    let mut events = Vec::new();
    let mut close = false;

    let modal = Modal::new(Id::new("settings-account-signout-modal")).show(ui.ctx(), |ui| {
        ui.heading(tr("signout-confirm-title"));
        ui.label(tr("signout-confirm-intro"));
        for category in &categories {
            ui.label(format!("• {}", tr(category)));
        }
        ui.horizontal(|ui| {
            let cancel = ui.button(tr("signout-cancel"));
            if focus_pending {
                cancel.request_focus();
                focus_pending = false;
            }
            if cancel.clicked() {
                close = true;
            }
            let sign_out = ui.add(egui::Button::new(
                RichText::new(tr("signout-confirm")).color(ui.visuals().error_fg_color),
            ));
            if sign_out.clicked() {
                events = account.sign_out();
                close = true;
            }
        });
    });

    ui.memory_mut(|memory| {
        memory
            .data
            .insert_temp(modal_focus_pending_id(), focus_pending)
    });
    if modal.should_close() {
        close = true;
    }
    if close {
        close_sign_out_modal(ui);
    }

    events
}

/// Test-only: open and draw the sign-out confirmation modal directly,
/// without a live click on "Sign out" first (`shell.rs`'s accessibility
/// sweep, T106, needs the modal's Cancel/Sign out buttons in their open
/// state; `show_signed_in` only reaches `show_sign_out_modal` once
/// `account.account()` is `Some`, which the sweep's plain `SignedOut`
/// fixture never is).
#[cfg(test)]
pub(crate) fn show_sign_out_modal_for_test(
    ui: &mut Ui,
    account: &mut AccountService,
) -> Vec<AccountEvent> {
    open_sign_out_modal(ui);
    show_sign_out_modal(ui, account)
}

fn tier_key(tier: Tier) -> &'static str {
    match tier {
        Tier::Premium => "tier-premium",
        Tier::Free => "tier-free",
        Tier::Unknown => "tier-unknown",
    }
}

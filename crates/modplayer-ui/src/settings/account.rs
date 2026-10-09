// SPDX-License-Identifier: MIT OR Apache-2.0

//! Settings › Account (contracts/ui-surface.md "Settings › Account"): the
//! signed-in view (display name, tier, last validated, Re-check
//! subscription — T067), the sign-out confirmation modal (T085), and the
//! signed-out view (T087). Returns any `AccountEvent`s a command (Sign in,
//! Sign out) raised this frame so `App::handle_account_event` (US2 T071 /
//! US3 T086) can map them to notifications/screens exactly like the
//! sign-in step's own commands do.

use chrono::{Local, Offset, TimeZone};
use egui::{Id, Modal, RichText, Ui};
use modplayer_account::{AccountEvent, AccountService, SessionState, Tier};
use modplayer_core::{tr, tr_args};
use time::{OffsetDateTime, UtcOffset};

use crate::settings::field;
use crate::theme;
use crate::theme::controls::Variant;
use crate::theme::tokens::{self, space, text};
use crate::widgets::controls::{button, panel_card};

/// Draw the Settings › Account screen for the current session state.
///
/// `focus` names the descriptor id (`account.recheck_subscription` or
/// `account.sign_out`), if any, a settings-search result asked to focus and
/// highlight this frame (028, A8).
pub fn show(ui: &mut Ui, account: &mut AccountService, focus: Option<&str>) -> Vec<AccountEvent> {
    match account.state() {
        SessionState::Active | SessionState::Expired => show_signed_in(ui, account, focus),
        _ => show_signed_out(ui, account),
    }
}

/// Outline `response`'s button when `id` is the search-highlight target and
/// move focus to it when a search result just asked for it (A8).
fn highlight_button(ui: &mut Ui, response: &egui::Response, id: &str, focus: Option<&str>) {
    if focus == Some(id) {
        response.request_focus();
    }
    if let Some(armed) = field::highlight_state(ui.ctx(), id) {
        field::paint_highlight(ui, response.rect, !armed);
    }
}

fn show_signed_in(
    ui: &mut Ui,
    account: &mut AccountService,
    focus: Option<&str>,
) -> Vec<AccountEvent> {
    let Some(session) = account.account() else {
        // A7: Active/Expired without an account never shows placeholder text.
        return show_signed_out(ui, account);
    };

    let identity = identity_text(&session.display_name);
    let tier_value = tier_key(session.tier);
    let last_verified = match session.last_validated_at {
        Some(when) => format_last_verified(when, local_offset_at(when)),
        None => tr("account-never-verified"),
    };
    summary_card(ui, &identity, &tr(tier_value), &last_verified);
    ui.add_space(space::LG);

    // FR-006, U2: field-description prose, capped at the 72-character
    // measure (research R17).
    ui.scope(|ui| {
        ui.set_max_width(ui.available_width().min(theme::body_measure(ui.ctx())));
        ui.label(tr("account-recheck-desc"));
    });
    let recheck = ui.button(tr("account-recheck"));
    highlight_button(ui, &recheck, "account.recheck_subscription", focus);
    if recheck.clicked() {
        account.recheck_tier();
    }

    theme::divider(ui);

    let sign_out = button(ui, Variant::Destructive, tr("account-sign-out"));
    highlight_button(ui, &sign_out, "account.sign_out", focus);
    if sign_out.clicked() {
        open_sign_out_modal(ui);
    }

    show_sign_out_modal(ui, account)
}

/// The summary card (A1, A2): identity in strong body text, then the tier
/// and last-verified lines as a secondary label beside its value.
fn summary_card(ui: &mut Ui, identity: &str, tier: &str, last_verified: &str) {
    let roles = tokens::roles(ui.visuals());
    panel_card(ui, &tr("account-summary-title"), |ui| {
        ui.label(RichText::new(identity).strong());
        for (label_key, value) in [
            ("account-tier", tier),
            ("account-last-verified", last_verified),
        ] {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(tr(label_key))
                        .text_style(text::SECONDARY)
                        .color(roles.text_secondary),
                );
                ui.label(value);
            });
        }
    });
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

/// Smallest sign-out dialog width, in logical pixels (S1).
const SIGNOUT_DIALOG_MIN_W: f32 = 420.0;
/// Largest sign-out dialog width, in logical pixels (S1).
const SIGNOUT_DIALOG_MAX_W: f32 = 560.0;

/// Width of the sign-out confirmation dialog for a viewport `viewport_w`
/// pixels wide: `clamp(viewport_w − 2·space::XXL, 420, 560)` (S1).
///
/// ```
/// # // pub(crate): mirrored by the unit test below.
/// let w = |v: f32| (v - 64.0).clamp(420.0, 560.0);
/// assert_eq!(w(960.0), 560.0);
/// assert_eq!(w(500.0), 436.0);
/// assert_eq!(w(400.0), 420.0);
/// ```
pub(crate) fn signout_dialog_width(viewport_w: f32) -> f32 {
    (viewport_w - 2.0 * space::XXL).clamp(SIGNOUT_DIALOG_MIN_W, SIGNOUT_DIALOG_MAX_W)
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

    let width = signout_dialog_width(ui.ctx().content_rect().width());
    let modal = Modal::new(Id::new("settings-account-signout-modal")).show(ui.ctx(), |ui| {
        ui.set_width(width);
        ui.add(egui::Label::new(RichText::new(tr("signout-confirm-title")).heading()).wrap());
        ui.add(egui::Label::new(tr("signout-confirm-intro")).wrap());
        for category in &categories {
            // Hanging indent: the bullet sits in its own column so wrapped
            // lines align under the text, not under the bullet.
            ui.horizontal_top(|ui| {
                ui.label("•");
                ui.add(egui::Label::new(tr(category)).wrap());
            });
        }
        ui.horizontal(|ui| {
            let cancel = ui.button(tr("signout-cancel"));
            // Keep asking until Cancel actually holds focus: a request made
            // on the opening frame can be overridden later in that frame
            // (manual walk M7). Once it has landed, the user may move it.
            if focus_pending {
                if cancel.has_focus() {
                    focus_pending = false;
                } else {
                    cancel.request_focus();
                }
            }
            if cancel.clicked() {
                close = true;
            }
            let sign_out = button(ui, Variant::Destructive, tr("signout-confirm"));
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

/// Fluent key for the tier value on the summary card. `Unknown` reads "Not
/// verified yet" (A2), not the generic `tier-unknown`.
fn tier_key(tier: Tier) -> &'static str {
    match tier {
        Tier::Premium => "tier-premium",
        Tier::Free => "tier-free",
        Tier::Unknown => "account-tier-unverified",
    }
}

/// The identity line: the display name, or "Name unavailable" when it is
/// empty or only whitespace, so the card never shows a blank line (A3).
fn identity_text(display_name: &str) -> String {
    if display_name.trim().is_empty() {
        tr("account-identity-unavailable")
    } else {
        display_name.to_string()
    }
}

/// The machine's local UTC offset at `when`, falling back to UTC when the
/// platform cannot resolve one. Never panics.
///
/// ```
/// use modplayer_ui::settings::account::local_offset_at;
/// let offset = local_offset_at(time::OffsetDateTime::UNIX_EPOCH);
/// assert!(offset.whole_hours().abs() <= 26);
/// ```
pub fn local_offset_at(when: OffsetDateTime) -> UtcOffset {
    Local
        .timestamp_opt(when.unix_timestamp(), 0)
        .single()
        .and_then(|local| {
            UtcOffset::from_whole_seconds(local.offset().fix().local_minus_utc()).ok()
        })
        .unwrap_or(UtcOffset::UTC)
}

/// Format `when` for the "Last verified" line in `offset`, for example
/// `30 Sep 2026, 17:59`: day without padding, short month from the
/// `date-month-short-N` strings, year, then a zero-padded 24-hour `HH:MM`.
/// Never the RFC 3339 form (A4).
///
/// ```
/// use modplayer_ui::settings::account::format_last_verified;
/// use time::{Date, Month, UtcOffset};
/// let when = Date::from_calendar_date(2026, Month::September, 30)
///     .unwrap()
///     .with_hms(20, 59, 0)
///     .unwrap()
///     .assume_utc();
/// let offset = UtcOffset::from_hms(-3, 0, 0).unwrap();
/// assert_eq!(format_last_verified(when, offset), "30 Sep 2026, 17:59");
/// ```
pub fn format_last_verified(when: OffsetDateTime, offset: UtcOffset) -> String {
    let local = when.to_offset(offset);
    field::strip_isolates(tr_args(
        "account-last-verified-at",
        &[
            ("day", local.day().to_string()),
            (
                "month",
                tr(&format!("date-month-short-{}", u8::from(local.month()))),
            ),
            ("year", local.year().to_string()),
            ("time", format!("{:02}:{:02}", local.hour(), local.minute())),
        ],
    ))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::disallowed_methods)]
mod tests {
    use super::*;
    use time::{Date, Month, OffsetDateTime, UtcOffset};

    #[test]
    fn signout_dialog_width_clamps_to_420_560() {
        assert_eq!(signout_dialog_width(960.0), 560.0);
        assert_eq!(signout_dialog_width(500.0), 436.0);
        assert_eq!(signout_dialog_width(400.0), 420.0);
    }

    fn utc(y: i32, month: Month, d: u8, h: u8, m: u8) -> OffsetDateTime {
        Date::from_calendar_date(y, month, d)
            .expect("valid date")
            .with_hms(h, m, 0)
            .expect("valid time")
            .assume_utc()
    }

    fn offset(hours: i8) -> UtcOffset {
        UtcOffset::from_hms(hours, 0, 0).expect("valid offset")
    }

    /// A4: day (unpadded), short month, year, 24 h `HH:MM` in the given offset.
    #[test]
    fn format_last_verified_uses_the_given_offset() {
        let when = utc(2026, Month::September, 30, 20, 59);
        assert_eq!(format_last_verified(when, offset(-3)), "30 Sep 2026, 17:59");
    }

    /// A4: the local day can differ from the UTC day.
    #[test]
    fn format_last_verified_rolls_the_day_over() {
        let when = utc(2026, Month::September, 30, 23, 30);
        assert_eq!(format_last_verified(when, offset(2)), "1 Oct 2026, 01:30");
        let when = utc(2026, Month::January, 1, 1, 0);
        assert_eq!(format_last_verified(when, offset(-3)), "31 Dec 2025, 22:00");
    }

    /// A4: midnight reads `00:05`; the day is not zero padded, the time is.
    #[test]
    fn format_last_verified_pads_time_not_day() {
        let when = utc(2026, Month::September, 5, 3, 5);
        assert_eq!(format_last_verified(when, offset(-3)), "5 Sep 2026, 00:05");
        let when = utc(2026, Month::March, 9, 9, 7);
        assert_eq!(
            format_last_verified(when, UtcOffset::UTC),
            "9 Mar 2026, 09:07"
        );
    }

    /// A4: never the RFC 3339 / `Display` form.
    #[test]
    fn format_last_verified_is_not_the_display_form() {
        let when = utc(2026, Month::September, 30, 20, 59);
        let text = format_last_verified(when, UtcOffset::UTC);
        assert!(!text.contains('T'), "{text}");
        assert!(!text.contains("+00"), "{text}");
        assert!(
            !text.contains(':') || text.matches(':').count() == 1,
            "{text}"
        );
    }

    /// A3: empty and whitespace-only names fall back, never a blank line.
    #[test]
    fn identity_falls_back_when_blank() {
        let fallback = tr("account-identity-unavailable");
        assert_eq!(identity_text(""), fallback);
        assert_eq!(identity_text("   \t"), fallback);
        assert_eq!(identity_text("Alex"), "Alex");
    }

    /// A2: tier values, with `Unknown` reading "Not verified yet".
    #[test]
    fn tier_value_keys() {
        assert_eq!(tier_key(Tier::Premium), "tier-premium");
        assert_eq!(tier_key(Tier::Free), "tier-free");
        assert_eq!(tier_key(Tier::Unknown), "account-tier-unverified");
    }

    /// The local offset lookup never panics and yields a valid offset.
    #[test]
    fn local_offset_is_valid() {
        let when = utc(2026, Month::September, 30, 20, 59);
        let offset = local_offset_at(when);
        assert!(offset.whole_hours().abs() <= 26);
    }
}

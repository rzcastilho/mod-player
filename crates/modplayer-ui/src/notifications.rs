// SPDX-License-Identifier: MIT OR Apache-2.0

//! The notification area (contracts/ui-surface.md "Main window"): a
//! newest-first stack, each item a severity icon, severity text, message,
//! and Dismiss button. `Info` items auto-dismiss via
//! `NotificationCenter::tick` (called once per frame by the caller); this
//! widget never wraps itself in a modal window and never disables any
//! other widget — the caller (`app.rs`) draws it as a floating, non-
//! blocking overlay so it never interrupts playback or navigation.
//!
//! Colour never carries meaning alone (Constitution/FR-022): each item's
//! severity is also spelled out as text, not just an icon/colour.

use std::collections::BTreeSet;

use egui::{
    Color32, CornerRadius, Frame, Margin, RichText, ScrollArea, Ui, WidgetInfo, WidgetType, vec2,
};
use modplayer_core::{Notification, NotificationAction, NotificationCenter, Severity, tr, tr_args};

use crate::plugin_assets;
use crate::theme::{self, Roles};

/// The most cards the collapsed stack ever renders before an "{N} more"
/// control takes over (contract S2, data-model.md §6).
pub const MAX_COLLAPSED_CARDS: usize = 3;

/// How many of `center.visible()` a frame renders (`shown`), and how many
/// sit behind the overflow control (`overflow`) — see [`partition`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StackPartition {
    pub shown: usize,
    pub overflow: usize,
}

/// Pure partition of `visible_len` visible notifications into `shown` +
/// `overflow`, given whether the stack is currently expanded (research R3,
/// contract S2/S3, data-model.md §6): collapsed shows at most
/// `MAX_COLLAPSED_CARDS`; expanded shows all of them. Recomputed fresh
/// every frame from `NotificationCenter::visible()`, so a dismissal
/// promotes the newest hidden card "for free" (FR-006) — no separate
/// bookkeeping.
///
/// # Examples
///
/// ```
/// use modplayer_ui::notifications::{partition, StackPartition, MAX_COLLAPSED_CARDS};
///
/// // Collapsed: capped at MAX_COLLAPSED_CARDS, the rest overflow.
/// assert_eq!(
///     partition(4, false),
///     StackPartition { shown: MAX_COLLAPSED_CARDS, overflow: 1 }
/// );
///
/// // Expanded: everything shows; `overflow` still reports how many sit
/// // past the collapsed cap, so the "Show fewer" caption can name it.
/// assert_eq!(partition(4, true), StackPartition { shown: 4, overflow: 1 });
///
/// // Fewer than the cap: shown tracks the count either way.
/// assert_eq!(partition(2, false), StackPartition { shown: 2, overflow: 0 });
/// ```
pub fn partition(visible_len: usize, expanded: bool) -> StackPartition {
    let shown = if expanded {
        visible_len
    } else {
        visible_len.min(MAX_COLLAPSED_CARDS)
    };
    StackPartition {
        shown,
        overflow: visible_len.saturating_sub(MAX_COLLAPSED_CARDS),
    }
}

/// The card width at a given inner window width (contract S4, data-model.md
/// §6): capped at 360 logical px, shrinking to fit a 16 px margin on
/// narrower windows.
///
/// # Examples
///
/// ```
/// use modplayer_ui::notifications::card_width;
///
/// // Wide window: capped at 360 px.
/// assert_eq!(card_width(1200.0), 360.0);
///
/// // Narrow window: shrinks to leave a 16 px margin.
/// assert_eq!(card_width(300.0), 284.0);
/// ```
pub fn card_width(screen_width: f32) -> f32 {
    (screen_width - 16.0).min(360.0)
}

/// The notification stack's own UI-only presentation state (data-model.md
/// §6), owned by the app (`ModPlayerApp.notification_stack`) rather than
/// egui memory (research R5) so it is directly assertable in tests and
/// survives `Area` id changes. Never persisted.
///
/// # Examples
///
/// ```
/// use modplayer_ui::notifications::StackState;
///
/// // The app owns one instance for the lifetime of the window; it starts
/// // fully collapsed, with no card's "Show more" or "Details" expanded.
/// let state = StackState::default();
/// assert_eq!(format!("{state:?}"), "StackState { expanded: false, show_more: {}, details: {} }");
/// ```
#[derive(Debug, Default)]
pub struct StackState {
    /// Whether the "{N} more" control has been activated; forced back to
    /// `false` once there is nothing left to overflow (FR-006).
    expanded: bool,
    /// Ids whose full (untruncated) message is shown (US4, contract S5).
    show_more: BTreeSet<u64>,
    /// Ids whose `detail` payload is shown (US4, contract S6).
    details: BTreeSet<u64>,
}

/// What the user did with the notification stack this frame, if anything
/// (002-first-launch-and-sign-in contracts/ui-surface.md "notifications
/// with actions").
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NotificationInteraction {
    /// The notification the user clicked Dismiss on, if any — the caller
    /// applies it via `NotificationCenter::dismiss`.
    pub dismissed: Option<u64>,
    /// The notification whose action button the user clicked, and which
    /// action it was (e.g. `session-expired`'s "Sign in") — the caller
    /// reacts to it (starting US2 T071; this phase only renders the
    /// button).
    pub action_clicked: Option<(u64, NotificationAction)>,
    /// Every `Info` card whose "Show more" or "Details" expansion started
    /// (`true`) or ended (`false`) this frame (US4, FR-009, contract S7):
    /// the caller applies each as `hold_auto_dismiss`/`release_auto_dismiss`
    /// on the `NotificationCenter` (research R8).
    pub hold_changes: Vec<(u64, bool)>,
}

/// Draw every visible (non-dismissed) notification, newest first: a
/// severity icon+text, the message, up to `MAX_NOTIFICATION_ACTIONS`
/// action buttons (`notification.actions`, e.g. `stream-source-
/// unavailable`'s **Open status page** + **Retry**, US4 T090), and
/// Dismiss.
///
/// Each item sits in an opaque popup-style frame: the caller floats this
/// stack over the current screen, so without a background the text would
/// collide with whatever is underneath (found by quickstart M4.2).
pub fn show(
    ui: &mut Ui,
    center: &NotificationCenter,
    state: &mut StackState,
) -> NotificationInteraction {
    let mut interaction = NotificationInteraction::default();
    let visible: Vec<&Notification> = center.visible().collect();
    let StackPartition { shown, overflow } = partition(visible.len(), state.expanded);
    // FR-006 "returns to collapsed": nothing left to overflow, so the
    // "Show fewer" control (and the expanded layout it controls) goes away
    // on its own, even if the user never clicked it.
    if overflow == 0 {
        state.expanded = false;
    }

    // data-model.md §6: prune expansion state for any id that is no longer
    // visible (dismissed, expired, …) so it never resurrects stale flags —
    // ids that merely moved between shown/overflow keep their flags.
    let visible_ids: BTreeSet<u64> = visible.iter().map(|n| n.id).collect();
    state.show_more.retain(|id| visible_ids.contains(id));
    state.details.retain(|id| visible_ids.contains(id));

    let width = card_width(ui.ctx().content_rect().width());

    if state.expanded {
        // R4: the "Show fewer" button sits outside the scroll area so it
        // is always reachable, however tall the expanded list gets.
        let max_height = 0.6 * ui.ctx().content_rect().height();
        ScrollArea::vertical()
            .max_height(max_height)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for notification in visible.iter().take(shown).copied() {
                    card(ui, notification, state, width, &mut interaction);
                }
            });
        if ui.button(tr("notification-show-fewer")).clicked() {
            state.expanded = false;
        }
    } else {
        for notification in visible.iter().take(shown).copied() {
            card(ui, notification, state, width, &mut interaction);
        }
        if overflow > 0 {
            let label = tr_args("notification-more", &[("count", overflow.to_string())]);
            if ui.button(label).clicked() {
                state.expanded = true;
            }
        }
    }

    interaction
}

/// Render one notification's card (contract S4): a 4 px accent bar + a
/// coloured severity icon (both in the severity's role colour), the
/// severity word, the message (truncated to two lines, contract S5), an
/// optional "Details" payload (contract S6), up to
/// `MAX_NOTIFICATION_ACTIONS` action buttons, and Dismiss.
fn card(
    ui: &mut Ui,
    notification: &Notification,
    state: &mut StackState,
    width: f32,
    interaction: &mut NotificationInteraction,
) {
    let roles = theme::roles(ui.visuals());
    let color = severity_color(roles, notification.severity);
    let id = notification.id;

    // R7/S4: `space::SM` padding on every side, with the left edge widened
    // by the 4 px accent bar painted after the frame (so the bar sits in
    // the extra margin, never over the text).
    let mut margin = Margin::same(theme::space::SM as i8);
    margin.left = margin.left.saturating_add(4);
    let content_width = (width - f32::from(margin.left) - f32::from(margin.right)).max(0.0);

    // S7/R8: whether this card currently wants its `Info` auto-dismiss
    // timer held, before this frame's toggles are applied.
    let held_before = notification.severity == Severity::Info
        && (state.show_more.contains(&id) || state.details.contains(&id));

    let frame_response = Frame::new()
        .fill(roles.surface_raised)
        .inner_margin(margin)
        .corner_radius(theme::radius::MD)
        .show(ui, |ui| {
            ui.set_width(content_width);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    // FR-007: the icon glyph alone carries the severity colour.
                    ui.label(RichText::new(severity_glyph(notification.severity)).color(color));
                    // FR-008: the severity word stays as plain, uncoloured
                    // text next to the icon — colour/icon are never the
                    // sole carriers.
                    ui.label(tr(severity_key(notification.severity)));
                    // US5 N3/019 S4: attribution icon + name stay on this
                    // first row, before the message.
                    if let Some(attribution) = &notification.attribution {
                        plugin_assets::show_cached_icon_or_generic(ui, attribution.id, 16.0);
                        ui.label(&attribution.name);
                    }
                });

                message_row(ui, notification, state);

                if let Some(detail) = &notification.detail {
                    details_row(ui, id, detail, state);
                }

                ui.horizontal(|ui| {
                    for action in &notification.actions {
                        if ui.button(tr(action_label_key(*action))).clicked() {
                            interaction.action_clicked = Some((notification.id, *action));
                        }
                    }
                    if ui.button(tr("notification-dismiss")).clicked() {
                        interaction.dismissed = Some(notification.id);
                    }
                });
            });
        });

    // R7: paint the 4 px full-height accent bar on the frame's left edge,
    // left corners rounded to the card radius, right corners square (the
    // bar hugs the card's own left edge, not a rounded pill).
    let rect = frame_response.response.rect;
    let bar_rect = egui::Rect::from_min_size(rect.min, vec2(4.0, rect.height()));
    let bar_radius = CornerRadius {
        nw: theme::radius::MD.nw,
        ne: 0,
        sw: theme::radius::MD.sw,
        se: 0,
    };
    ui.painter().rect_filled(bar_rect, bar_radius, color);

    // S7/R8: report a hold-state transition, if any, so the caller applies
    // it to the `NotificationCenter` (WCAG 2.2.1).
    let held_after = notification.severity == Severity::Info
        && (state.show_more.contains(&id) || state.details.contains(&id));
    if held_after != held_before {
        interaction.hold_changes.push((id, held_after));
    }
}

/// The message row (contract S5, research R6): laid out at up to two rows
/// with a `…` ellipsis, "Show more"/"Show less" present iff the two-row
/// galley is elided (or already expanded); the label's AccessKit name is
/// always the *full*, untruncated resolved text (contract S8, FR-015),
/// regardless of what is visibly shown.
///
/// Plugin-attributed notifications only ever show the message *text*, not
/// the "<plugin>: " prefix (the attribution already rendered on the row
/// above) — the AccessKit name still carries the full resolved sentence.
fn message_row(ui: &mut Ui, notification: &Notification, state: &mut StackState) {
    let id = notification.id;
    let resolved = message(notification);
    let displayed_text: &str = if notification.attribution.is_some() {
        // Attribution icon + name already rendered on the row above; only
        // the message text itself is capped here.
        notification
            .args
            .iter()
            .find(|(key, _)| *key == "text")
            .map_or(resolved.as_str(), |(_, value)| value.as_str())
    } else {
        resolved.as_str()
    };

    let expanded = state.show_more.contains(&id);
    let text_width = ui.available_width();
    let font_id = egui::TextStyle::Body.resolve(ui.style());
    let text_color = ui.visuals().text_color();
    let mut job =
        egui::text::LayoutJob::simple(displayed_text.to_string(), font_id, text_color, text_width);
    job.wrap.max_rows = if expanded { usize::MAX } else { 2 };
    job.wrap.break_anywhere = false;
    job.wrap.overflow_character = Some('…');
    let galley = ui.ctx().fonts_mut(|f| f.layout_job(job));
    let elided = galley.elided;

    let response = ui.label(galley);
    // FR-015/contract S8: the accessible name is always the full resolved
    // text, whatever is visibly truncated.
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, resolved.clone()));

    if elided || expanded {
        let key = if expanded {
            "notification-show-less"
        } else {
            "notification-show-more"
        };
        if ui.button(tr(key)).clicked() {
            if expanded {
                state.show_more.remove(&id);
            } else {
                state.show_more.insert(id);
            }
        }
    }
}

/// The "Details"/"Hide details" toggle and payload (contract S6, research
/// R13): present iff `detail.is_some()` (the caller only calls this when it
/// is); the payload is selectable monospace text, independent of "Show
/// more".
fn details_row(ui: &mut Ui, id: u64, detail: &str, state: &mut StackState) {
    let open = state.details.contains(&id);
    let key = if open {
        "notification-hide-details"
    } else {
        "notification-details"
    };
    if ui.button(tr(key)).clicked() {
        if open {
            state.details.remove(&id);
        } else {
            state.details.insert(id);
        }
    }
    if state.details.contains(&id) {
        ui.add(
            egui::Label::new(RichText::new(detail).monospace())
                .selectable(true)
                .wrap(),
        );
    }
}

/// The severity's role colour (research R7, FR-007): `Info` → `positive`,
/// `Warning` → `warning`, `Critical` → `danger`. Read from `theme::roles`
/// (light/dark/high-contrast all covered), never a colour literal.
fn severity_color(roles: &Roles, severity: Severity) -> Color32 {
    match severity {
        Severity::Info => roles.positive,
        Severity::Warning => roles.warning,
        Severity::Critical => roles.danger,
    }
}

/// The severity's decorative icon glyph (FR-007). Never the sole carrier of
/// severity — [`severity_key`]'s word is always rendered alongside it.
fn severity_glyph(severity: Severity) -> &'static str {
    match severity {
        Severity::Critical => "⛔",
        Severity::Warning => "⚠",
        Severity::Info => "ℹ",
    }
}

/// The Fluent key for the severity word (FR-008) kept as visible text on
/// every card, in every accessible name.
fn severity_key(severity: Severity) -> &'static str {
    match severity {
        Severity::Critical => "severity-critical",
        Severity::Warning => "severity-warning",
        Severity::Info => "severity-info",
    }
}

/// The Fluent key for an action's button label. `app.rs`'s notification-
/// area handler (US4 T090) dispatches each variant: `SignIn` starts a
/// fresh sign-in attempt (US2 T071); `OpenStatusPage`/`OpenUpgradePage`
/// open `modplayer_core::STATUS_PAGE_URL`/`modplayer_account::UPGRADE_URL`
/// (design note 10); `RetrySource` re-initialises the source.
fn action_label_key(action: NotificationAction) -> &'static str {
    match action {
        NotificationAction::SignIn => "notification-action-sign-in",
        NotificationAction::OpenStatusPage => "action-status-page",
        NotificationAction::RetrySource => "action-retry",
        NotificationAction::OpenUpgradePage => "action-open-upgrade-page",
        // 009 US4 (T108): click handling (`plugin_restart`/`plugin_disable`)
        // lives in `app.rs`'s own action-dispatch match, exactly like every
        // other action here.
        NotificationAction::RestartPlugin(_) => "notification-action-restart-plugin",
        NotificationAction::DisablePlugin(_) => "notification-action-disable-plugin",
    }
}

/// Resolve a notification's message, substituting its Fluent `args` when it
/// has any (e.g. `device-lost`'s `{ $device }`).
fn message(notification: &Notification) -> String {
    if notification.args.is_empty() {
        tr(notification.message_key)
    } else {
        tr_args(notification.message_key, &notification.args)
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    // R3/T005: `partition`'s invariants hold for every `visible_len` /
    // `expanded` combination, not just the handful `notification_stack.rs`
    // exercises through egui.
    proptest! {
        #[test]
        fn partition_shown_never_exceeds_visible_len(
            visible_len in 0usize..64,
            expanded in any::<bool>(),
        ) {
            let StackPartition { shown, .. } = partition(visible_len, expanded);
            prop_assert!(shown <= visible_len);
        }

        #[test]
        fn partition_collapsed_shows_at_most_the_cap(visible_len in 0usize..64) {
            let StackPartition { shown, .. } = partition(visible_len, false);
            prop_assert!(shown <= MAX_COLLAPSED_CARDS);
        }

        #[test]
        fn partition_overflow_matches_the_formula(
            visible_len in 0usize..64,
            expanded in any::<bool>(),
        ) {
            let StackPartition { overflow, .. } = partition(visible_len, expanded);
            prop_assert_eq!(overflow, visible_len.saturating_sub(MAX_COLLAPSED_CARDS));
        }
    }

    // R7/FR-008: the severity word resolves to non-empty visible text for
    // every severity — the card-level check that it is actually present in
    // the rendered accessible tree lives in `tests/accessibility.rs`.
    #[test]
    fn severity_key_resolves_to_non_empty_text_for_every_severity() {
        for severity in [Severity::Critical, Severity::Warning, Severity::Info] {
            assert!(!tr(severity_key(severity)).is_empty());
        }
    }

    // FR-007: each severity gets its own icon glyph, so colour is never the
    // only visual distinction between severities.
    #[test]
    fn severity_glyph_differs_for_every_severity() {
        let glyphs: Vec<&str> = [Severity::Critical, Severity::Warning, Severity::Info]
            .into_iter()
            .map(severity_glyph)
            .collect();
        assert_eq!(
            glyphs.len(),
            glyphs
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
        );
    }

    // R7/FR-007: `Info`/`Warning`/`Critical` each map to a distinct role,
    // and never to a colour literal — `roles` here are the theme's own
    // static tables, not ad-hoc values.
    #[test]
    fn severity_color_maps_to_the_right_role() {
        use crate::theme::tokens::{DARK, DARK_HIGH_CONTRAST, LIGHT, LIGHT_HIGH_CONTRAST};

        for roles in [&LIGHT, &DARK, &LIGHT_HIGH_CONTRAST, &DARK_HIGH_CONTRAST] {
            assert_eq!(severity_color(roles, Severity::Info), roles.positive);
            assert_eq!(severity_color(roles, Severity::Warning), roles.warning);
            assert_eq!(severity_color(roles, Severity::Critical), roles.danger);
        }
    }

    #[test]
    fn message_falls_back_to_the_plain_key_with_no_args() {
        let mut center = NotificationCenter::new();
        center.raise(Severity::Info, "no-output-devices");
        let notification = center
            .visible()
            .next()
            .unwrap_or_else(|| unreachable!("just raised"));
        assert_eq!(message(notification), tr("no-output-devices"));
    }

    #[test]
    fn action_label_key_resolves_a_fluent_key_for_every_action() {
        assert_eq!(
            action_label_key(NotificationAction::SignIn),
            "notification-action-sign-in"
        );
    }

    #[test]
    fn default_interaction_is_empty() {
        let interaction = NotificationInteraction::default();
        assert_eq!(interaction.dismissed, None);
        assert_eq!(interaction.action_clicked, None);
    }
}

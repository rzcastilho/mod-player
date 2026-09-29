// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Transport panel (010-transport-focus, contracts/
//! ui-transport-panel.md): opened via `T`/the header toggle beside
//! "Queue"/"Effects" in Now Playing, shows who currently holds transport
//! focus, the user's focus policy, and every `transport.control`-granted
//! plugin's row (with its request order, if pending), letting the user
//! give/take back focus and change policy in one click each. Follows 008's
//! `effects_view.rs` for placement/toggle and its own panel-header/row
//! conventions.

use egui::{Button, ComboBox, RichText, Ui, WidgetInfo, WidgetType};
use modplayer_audio_io::OutputBackend;
use modplayer_audio_source::SourceHost;
use modplayer_core::{FocusPolicy, FocusRow, NowPlayingPanel, PlaybackController, tr, tr_args};

use crate::theme;
use crate::widgets::controls::{CardResponse, collapsible_panel_card};

/// `T` (`HostAction::ToggleTransportPanel`, contracts/ui-transport-panel.md
/// §1): flips the persisted `[now_playing_panels] transport_open` flag
/// through the controller, so a keyboard toggle and the header switch stay
/// in sync and the state survives a restart
/// (016-list-row-and-panel-components, FR-019). Returns the panel's new
/// open state (021-transport-bar-and-panel-layout, contract R5, research
/// R6): `true` tells the caller to also request a reveal.
pub fn toggle_transport_panel<B: OutputBackend, H: SourceHost>(
    controller: &mut PlaybackController<B, H>,
) -> bool {
    let open = controller.now_playing_panel_open(NowPlayingPanel::Transport);
    let new_open = !open;
    controller.set_now_playing_panel_open(NowPlayingPanel::Transport, new_open);
    new_open
}

/// Draw the Transport panel: the shared collapsible card (016-list-row-
/// and-panel-components, FR-017/FR-018; 021-transport-bar-and-panel-
/// layout contracts/ui-now-playing-layout.md C1-C2, C4) with the holder/
/// policy/take-back header row, then one row per eligible plugin (or the
/// empty state). Rendered regardless of whether a track is loaded —
/// holder reads "host" and rows may be empty (contracts/ui-transport-
/// panel.md §1) — and, per 021 contract C4, always present, either open
/// or as a header-only collapsed card.
pub fn show<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    open: &mut bool,
) -> CardResponse {
    let view = controller.transport_focus_view();

    collapsible_panel_card(ui, &tr("transport-panel-title"), open, |ui| {
        ui.horizontal(|ui| {
            let holder_name = view
                .holder
                .as_ref()
                .map_or_else(|| tr("transport-holder-host"), |row| row.name.clone());
            // 014-design-tokens-and-type-scale (US2, T029): non-numeric
            // status text, `secondary`/`.weak()` — the numeric transport
            // readouts this panel doesn't have are US3's T039, not this
            // task's scope.
            ui.label(RichText::new(tr_args("transport-holder", &[("holder", holder_name)])).weak());

            show_policy_combo(ui, controller, view.policy);

            if ui
                .add_enabled(
                    view.holder.is_some(),
                    Button::new(tr("transport-take-back")),
                )
                .clicked()
            {
                controller.focus_take_back();
            }
        });

        if view.rows.is_empty() {
            ui.label(tr("transport-empty"));
            return;
        }

        for row in &view.rows {
            show_row(ui, controller, row);
        }
    })
}

/// The Focus policy `ComboBox` (contracts/ui-transport-panel.md §2, U1):
/// a change calls `controller.set_focus_policy` immediately — the holder
/// and pending queue are untouched (C7).
fn show_policy_combo<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    current: FocusPolicy,
) {
    let mut selected = current;
    ui.label(tr("transport-policy"));
    ComboBox::from_id_salt("transport-policy")
        .selected_text(tr(current.label_key()))
        .show_ui(ui, |ui| {
            for option in FocusPolicy::ALL {
                ui.selectable_value(&mut selected, option, tr(option.label_key()));
            }
        });
    if selected != current {
        controller.set_focus_policy(selected);
    }
}

/// One plugin's row (contracts/ui-transport-panel.md §2): name, a
/// "holds focus"/"requesting (n)" badge (or neither), and a "Give focus"
/// button disabled while it already holds. The wrapping `Frame::group`
/// carries the row's own accessible name (`transport-row-a11y`) so a
/// screen reader announces holder/requesting state without visiting every
/// child control.
fn show_row<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    row: &FocusRow,
) {
    let state = row_state_text(row);
    let group = ui.group(|ui| {
        ui.horizontal(|ui| {
            // 014-design-tokens-and-type-scale (US2, T029): the row's own
            // plugin name is the row's `body`/`text_primary` title (mirrors
            // `rows::title_text`); the holds/requesting badge is its
            // secondary detail, `.weak()`, visually paired the same way.
            ui.label(
                RichText::new(&row.name)
                    .text_style(theme::text::BODY)
                    .color(theme::roles(ui.visuals()).text_primary),
            );
            if row.holds {
                ui.label(RichText::new(tr("transport-holds")).weak());
            } else if let Some(order) = row.request_order {
                // 014-design-tokens-and-type-scale (US3, T039): the request
                // order is this panel's one numeric readout — `mono` so its
                // digits sit in a fixed-width column across rows, layered
                // on top of the existing `.weak()` secondary weight.
                ui.label(
                    theme::mono_text(tr_args(
                        "transport-requesting",
                        &[("order", order.to_string())],
                    ))
                    .weak(),
                );
            }

            if ui
                .add_enabled(
                    !row.holds,
                    Button::new(tr_args(
                        "transport-give-focus",
                        &[("plugin", row.name.clone())],
                    )),
                )
                .clicked()
            {
                controller.focus_give(row.id);
            }
        });
    });
    group.response.widget_info(|| {
        WidgetInfo::labeled(
            WidgetType::Other,
            true,
            tr_args(
                "transport-row-a11y",
                &[("plugin", row.name.clone()), ("state", state.clone())],
            ),
        )
    });
}

/// This row's `$state` segment for `transport-row-a11y`: "holds focus",
/// "requesting (n)", or empty (a plain, non-requesting observer).
fn row_state_text(row: &FocusRow) -> String {
    if row.holds {
        tr("transport-holds")
    } else if let Some(order) = row.request_order {
        tr_args("transport-requesting", &[("order", order.to_string())])
    } else {
        String::new()
    }
}

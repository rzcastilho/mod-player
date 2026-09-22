// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Transport panel (010-transport-focus, contracts/
//! ui-transport-panel.md): opened via `T`/the header toggle beside
//! "Queue"/"Effects" in Now Playing, shows who currently holds transport
//! focus, the user's focus policy, and every `transport.control`-granted
//! plugin's row (with its request order, if pending), letting the user
//! give/take back focus and change policy in one click each. Follows 008's
//! `effects_view.rs` for placement/toggle and its own panel-header/row
//! conventions.

use egui::{Button, ComboBox, Id, RichText, Ui, WidgetInfo, WidgetType};
use modplayer_audio_io::OutputBackend;
use modplayer_audio_source::SourceHost;
use modplayer_core::{FocusPolicy, FocusRow, PlaybackController, tr, tr_args};

use crate::theme;

/// Persists the Transport panel's open/closed state across frames in
/// egui's own per-viewer memory (mirrors `effects_view::panel_open_id`):
/// survives track changes, dropped with the process.
pub fn panel_open_id() -> Id {
    Id::new("now-playing-transport-open")
}

/// `T` (`HostAction::ToggleTransportPanel`, contracts/ui-transport-panel.md
/// §1): flips the same egui temp-memory flag the header's "Transport"
/// toggle button reads/writes, so a keyboard toggle and a click stay in
/// sync.
pub fn toggle_transport_panel(ctx: &egui::Context) {
    let id = panel_open_id();
    ctx.memory_mut(|memory| {
        let open = memory.data.get_temp::<bool>(id).unwrap_or(false);
        memory.data.insert_temp(id, !open);
    });
}

/// Draw the Transport panel: title, holder/policy/take-back header row,
/// then one row per eligible plugin (or the empty state). Rendered
/// regardless of whether a track is loaded — holder reads "host" and rows
/// may be empty (contracts/ui-transport-panel.md §1).
pub fn show<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
) {
    let view = controller.transport_focus_view();

    ui.heading(tr("transport-panel-title"));

    ui.horizontal(|ui| {
        let holder_name = view
            .holder
            .as_ref()
            .map_or_else(|| tr("transport-holder-host"), |row| row.name.clone());
        // 014-design-tokens-and-type-scale (US2, T029): non-numeric status
        // text, `secondary`/`.weak()` — the numeric transport readouts
        // this panel doesn't have are US3's T039, not this task's scope.
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

// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Queue panel (contracts/ui-surface.md §2): a shuffle toggle and
//! repeat-cycle button in the header, rows in effective order (current
//! item, then the play-next block, then upcoming context — history never
//! shown) with origin badges and a current marker, and keyboard-operable
//! Move up/Move down/Play next/Remove actions per row. Reached from Now
//! Playing via the `queue-toggle` button (`now_playing.rs`).

use egui::{RichText, Ui};
use modplayer_audio_io::OutputBackend;
use modplayer_audio_source::{Repeat, SourceHost};
use modplayer_core::{Origin, PlaybackController, tr, tr_args};

use crate::theme;
use crate::theme::controls::Variant;
use crate::widgets::controls::{SwitchKind, button, panel_card, row_frame, switch};

/// Draw the Queue panel — the shared card
/// (016-list-row-and-panel-components, FR-017/FR-018) with a new
/// `queue-panel-title` header — applying any header/row action directly
/// to `controller`.
pub fn show<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
) {
    let view = controller.queue_view();

    panel_card(ui, &tr("queue-panel-title"), |ui| {
        ui.horizontal(|ui| {
            let mut shuffle = view.shuffle;
            if switch(ui, SwitchKind::Toggle, &mut shuffle, &tr("queue-shuffle")).changed() {
                controller.set_shuffle(shuffle);
            }

            let repeat_key = match view.repeat {
                Repeat::Off => "queue-repeat-off",
                Repeat::One => "queue-repeat-one",
                Repeat::All => "queue-repeat-all",
            };
            if ui.button(tr(repeat_key)).clicked() {
                let next = match view.repeat {
                    Repeat::Off => Repeat::One,
                    Repeat::One => Repeat::All,
                    Repeat::All => Repeat::Off,
                };
                controller.set_repeat(next);
            }
        });

        if view.items.is_empty() {
            // FR-006, U2: empty-state copy, capped at the 72-character
            // measure (research R17).
            ui.scope(|ui| {
                ui.set_max_width(ui.available_width().min(theme::body_measure(ui.ctx())));
                ui.label(tr("queue-empty"));
            });
            return;
        }

        for row in &view.items {
            // FR-009, contract I6: the row's hover/pressed fill, reserved
            // and set beneath the row's own content — zero layout change.
            let row_id = ui.id().with(("queue-row", row.uid));
            row_frame(ui, row_id, |ui| {
                ui.horizontal(|ui| {
                    // 014-design-tokens-and-type-scale (US2, T027,
                    // data-model.md §6 "List row title"/"List row
                    // secondary line"): the row's own title is explicit
                    // `body`/`text_primary` (mirrors `rows::title_text`);
                    // the current marker, artist and badges are the row's
                    // secondary detail, `.weak()` like every other row's
                    // detail line — visually paired the same way.
                    if row.is_current {
                        ui.label(RichText::new(tr("queue-current")).weak());
                    }
                    ui.label(
                        RichText::new(tr_args("queue-row", &[("title", row.title.clone())]))
                            .text_style(theme::text::BODY)
                            .color(theme::roles(ui.visuals()).text_primary),
                    );
                    if !row.artist.is_empty() {
                        ui.label(RichText::new(row.artist.clone()).weak());
                    }
                    if row.origin == Origin::PlayNext {
                        ui.label(RichText::new(tr("queue-badge-play-next")).weak());
                    }
                    if row.unavailable {
                        ui.label(RichText::new(tr("queue-badge-unavailable")).weak());
                    }

                    if button(ui, Variant::Quiet, tr("queue-move-up")).clicked() {
                        controller.queue_move_up(row.uid);
                    }
                    if button(ui, Variant::Quiet, tr("queue-move-down")).clicked() {
                        controller.queue_move_down(row.uid);
                    }
                    if !row.is_current
                        && button(ui, Variant::Quiet, tr("queue-play-next")).clicked()
                    {
                        controller.queue_play_next(row.uid);
                    }
                    if button(ui, Variant::Quiet, tr("queue-remove")).clicked() {
                        controller.queue_remove(row.uid);
                    }
                });
            });
        }
    });
}

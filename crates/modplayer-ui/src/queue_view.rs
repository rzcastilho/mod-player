// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Queue panel (contracts/ui-surface.md §2): a shuffle toggle and
//! repeat-cycle button in the header, rows in effective order (current
//! item, then the play-next block, then upcoming context — history never
//! shown) with origin badges and a current marker, and keyboard-operable
//! Move up/Move down/Play next/Remove actions per row. Reached from Now
//! Playing via the `queue-toggle` button (`now_playing.rs`).

use egui::{Button, Ui};
use modplayer_audio_io::OutputBackend;
use modplayer_audio_source::{Repeat, SourceHost};
use modplayer_core::{Origin, PlaybackController, tr, tr_args};

/// Draw the Queue panel, applying any header/row action directly to
/// `controller`.
pub fn show<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
) {
    let view = controller.queue_view();

    ui.horizontal(|ui| {
        if ui
            .selectable_label(view.shuffle, tr("queue-shuffle"))
            .clicked()
        {
            controller.set_shuffle(!view.shuffle);
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
        ui.label(tr("queue-empty"));
        return;
    }

    for row in &view.items {
        ui.horizontal(|ui| {
            if row.is_current {
                ui.label(tr("queue-current"));
            }
            ui.label(tr_args("queue-row", &[("title", row.title.clone())]));
            if !row.artist.is_empty() {
                ui.label(row.artist.clone());
            }
            if row.origin == Origin::PlayNext {
                ui.label(tr("queue-badge-play-next"));
            }
            if row.unavailable {
                ui.label(tr("queue-badge-unavailable"));
            }

            if ui.add(Button::new(tr("queue-move-up"))).clicked() {
                controller.queue_move_up(row.uid);
            }
            if ui.add(Button::new(tr("queue-move-down"))).clicked() {
                controller.queue_move_down(row.uid);
            }
            if !row.is_current && ui.add(Button::new(tr("queue-play-next"))).clicked() {
                controller.queue_play_next(row.uid);
            }
            if ui.add(Button::new(tr("queue-remove"))).clicked() {
                controller.queue_remove(row.uid);
            }
        });
    }
}

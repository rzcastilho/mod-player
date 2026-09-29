// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Queue panel (contracts/ui-surface.md §2): a shuffle toggle and
//! repeat-cycle button in the header, rows in effective order (current
//! item, then the play-next block, then upcoming context — history never
//! shown) with origin badges and a current marker, and keyboard-operable
//! Move up/Move down/Play next/Remove actions per row. Reached from Now
//! Playing via the `queue-toggle` button (`now_playing.rs`).

use egui::Ui;
use modplayer_audio_io::OutputBackend;
use modplayer_audio_source::{Repeat, SourceHost};
use modplayer_core::{PlaybackController, tr};

use crate::artwork::ArtworkCache;
use crate::rows::{QueueRowAction, queue_row};
use crate::theme;
use crate::widgets::controls::{SwitchKind, switch};

/// Draw the Queue panel's body — header controls (shuffle, repeat) then
/// the rows — applying any header/row action directly to `controller`.
/// The card's own chrome (021-transport-bar-and-panel-layout, contract
/// C1, C4) is owned by the caller (`now_playing::show`, via
/// `widgets::controls::collapsible_panel_card`), superseding this
/// function's earlier direct `panel_card` call
/// (016-list-row-and-panel-components, FR-017/FR-018). Each row renders
/// through [`rows::queue_row`] (021 contracts/queue-row.md, research R8);
/// its returned [`QueueRowAction`] is applied to `controller` here, the
/// only place that owns it (design note 6).
pub fn show<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    artwork: &mut ArtworkCache,
) {
    let view = controller.queue_view();

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

    for (index, row) in view.items.iter().enumerate() {
        // Contract "Queue position" (spec Assumption): 1-based, the row's
        // place in this already-effective-order list.
        let position = index + 1;
        if let Some(action) = queue_row(ui, artwork, row, position) {
            match action {
                QueueRowAction::MoveUp => controller.queue_move_up(row.uid),
                QueueRowAction::MoveDown => controller.queue_move_down(row.uid),
                QueueRowAction::PlayNext => controller.queue_play_next(row.uid),
                QueueRowAction::Remove => controller.queue_remove(row.uid),
            }
        }
    }
}

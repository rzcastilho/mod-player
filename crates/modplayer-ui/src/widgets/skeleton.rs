// SPDX-License-Identifier: MIT OR Apache-2.0

//! The skeleton row widget (contracts/ui-surface.md §7): a row-shaped
//! shimmer with `Role::Status`, accessible name "Loading" (the `loading`
//! Fluent key) — the **only** loading affordance anywhere in this slice
//! (FR-016); no full-view spinner exists.

use egui::accesskit::Role;
use egui::{CornerRadius, Sense, Ui, Vec2};
use modplayer_core::tr;

/// Track-row height (research R9: "56 px track rows, 72 px album/artist/
/// playlist rows") — the default a caller renders while a row of unknown
/// kind is still loading.
pub const ROW_HEIGHT: f32 = 56.0;
/// Album/artist/playlist row height (research R9).
pub const WIDE_ROW_HEIGHT: f32 = 72.0;

/// Draw one skeleton row of `height`, `Role::Status` named `loading`
/// (contracts/ui-surface.md §7). Call once per loading row a list is
/// about to render.
pub fn skeleton_row(ui: &mut Ui, height: f32) {
    let size = Vec2::new(ui.available_width(), height);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::from(4u8), ui.visuals().faint_bg_color);
    }

    ui.ctx().accesskit_node_builder(response.id, |builder| {
        builder.set_role(Role::Status);
        builder.set_label(tr("loading"));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Context, RawInput};

    #[test]
    fn skeleton_row_reports_status_role_and_loading_label() {
        let ctx = Context::default();
        ctx.enable_accesskit();
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            skeleton_row(ui, ROW_HEIGHT);
        });
        let update = output
            .platform_output
            .accesskit_update
            .take()
            .unwrap_or_else(|| unreachable!("accesskit_update should be populated once enabled"));
        output.drop_without_applying_deltas();

        let found = update
            .nodes
            .iter()
            .any(|(_, node)| node.role() == Role::Status && node.label() == Some(&*tr("loading")));
        assert!(
            found,
            "expected a Role::Status node named `{}`",
            tr("loading")
        );
    }
}

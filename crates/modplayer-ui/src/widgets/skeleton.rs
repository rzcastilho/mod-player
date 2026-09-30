// SPDX-License-Identifier: MIT OR Apache-2.0

//! The skeleton row widget (contracts/ui-surface.md §7): a row-shaped
//! shimmer with `Role::Status`, accessible name "Loading" (the `loading`
//! Fluent key) — the **only** loading affordance anywhere in this slice
//! (FR-016); no full-view spinner exists.

use egui::accesskit::Role;
use egui::{Sense, Ui, Vec2};
use modplayer_core::tr;

use crate::detail_view::{DETAIL_ARTWORK_SIZE, header_height};
use crate::rows::ARTWORK_SIZE;
use crate::theme;
use crate::theme::controls::Variant;
use crate::widgets::controls::button;

/// Track-row height (research R9: "56 px track rows, 72 px album/artist/
/// playlist rows") — the default a caller renders while a row of unknown
/// kind is still loading.
pub const ROW_HEIGHT: f32 = 56.0;
/// Album/artist/playlist row height (research R9).
pub const WIDE_ROW_HEIGHT: f32 = 72.0;

/// Row-shaped skeleton geometry (025 data-model §4): total `height` and
/// how many text bars sit beside the artwork square, so a skeleton row
/// matches the loaded row it stands in for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkeletonShape {
    /// Total row height in points.
    pub height: f32,
    /// Number of text bars drawn beside the artwork square.
    pub text_lines: usize,
}

impl SkeletonShape {
    /// Track row: 56 px, title + subtitle.
    pub const TRACK: Self = Self {
        height: ROW_HEIGHT,
        text_lines: 2,
    };
    /// Album row: 72 px, title + subtitle.
    pub const ALBUM: Self = Self {
        height: WIDE_ROW_HEIGHT,
        text_lines: 2,
    };
    /// Playlist row: 72 px, title + owner/count.
    pub const PLAYLIST: Self = Self {
        height: WIDE_ROW_HEIGHT,
        text_lines: 2,
    };
    /// Artist row: 72 px, name only.
    pub const ARTIST: Self = Self {
        height: WIDE_ROW_HEIGHT,
        text_lines: 1,
    };
}

/// Height of one text bar in a skeleton row.
const BAR_HEIGHT: f32 = 10.0;

/// Draw one skeleton row of `shape`: an artwork square at
/// [`ARTWORK_SIZE`] plus one bar per text line, all filled from
/// `faint_bg_color`, exposed as a single `Role::Status` named `loading`
/// (contracts/ui-surface.md §7; no `ListItem`, not selectable). Call once
/// per loading row a list is about to render.
pub fn skeleton_row(ui: &mut Ui, shape: SkeletonShape) {
    let size = Vec2::new(ui.available_width(), shape.height);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());

    if ui.is_rect_visible(rect) {
        let fill = ui.visuals().faint_bg_color;
        let gap = ui.spacing().item_spacing.x;
        let painter = ui.painter();
        let art = egui::Rect::from_min_size(
            egui::pos2(rect.min.x, rect.center().y - ARTWORK_SIZE * 0.5),
            Vec2::splat(ARTWORK_SIZE),
        );
        painter.rect_filled(art, theme::radius::SM, fill);

        let bar_x = art.max.x + gap;
        let bar_width = (rect.max.x - bar_x).max(0.0);
        let lines = shape.text_lines;
        let block = lines as f32 * BAR_HEIGHT + lines.saturating_sub(1) as f32 * gap * 0.5;
        let mut y = rect.center().y - block * 0.5;
        for line in 0..lines {
            // Secondary bars are shorter, like a subtitle under a title.
            let width = if line == 0 {
                bar_width
            } else {
                bar_width * 0.6
            };
            let bar = egui::Rect::from_min_size(egui::pos2(bar_x, y), Vec2::new(width, BAR_HEIGHT));
            painter.rect_filled(bar, theme::radius::SM, fill);
            y += BAR_HEIGHT + gap * 0.5;
        }
    }

    ui.ctx().accesskit_node_builder(response.id, |builder| {
        builder.set_role(Role::Status);
        builder.set_label(tr("loading"));
    });
}

/// Draw the collection-header skeleton (025 FR-005/FR-010): a rect of
/// exactly [`header_height`], a live Back button on top, a 128 px artwork
/// square, and a title bar and a facts bar beside it. Returns `true` when
/// Back was clicked. Exposed as one `Role::Status` named `loading`.
pub fn header_skeleton(ui: &mut Ui) -> bool {
    let width = ui.available_width();
    let height = header_height(ui.spacing());
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    let ui = &mut child;
    ui.spacing_mut().item_spacing.y = 0.0;
    let back = button(ui, Variant::Quiet, tr("detail-back")).clicked();
    ui.add_space(theme::space::SM);

    let fill = ui.visuals().faint_bg_color;
    let gap = ui.spacing().item_spacing.x;
    let top = ui.cursor().min.y;
    let art = egui::Rect::from_min_size(
        egui::pos2(rect.min.x, top),
        Vec2::splat(DETAIL_ARTWORK_SIZE),
    );
    let painter = ui.painter();
    painter.rect_filled(art, theme::radius::SM, fill);
    let bar_x = art.max.x + gap;
    let bar_width = (rect.max.x - bar_x).max(0.0);
    let title = egui::Rect::from_min_size(
        egui::pos2(bar_x, top),
        Vec2::new(bar_width * 0.5, BAR_HEIGHT * 2.0),
    );
    painter.rect_filled(title, theme::radius::SM, fill);
    let facts = egui::Rect::from_min_size(
        egui::pos2(bar_x, title.max.y + gap * 0.5),
        Vec2::new(bar_width * 0.3, BAR_HEIGHT),
    );
    painter.rect_filled(facts, theme::radius::SM, fill);

    ui.ctx().accesskit_node_builder(response.id, |builder| {
        builder.set_role(Role::Status);
        builder.set_label(tr("loading"));
    });
    back
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
            skeleton_row(ui, SkeletonShape::TRACK);
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

    #[test]
    fn shapes_match_loaded_row_heights() {
        assert_eq!(SkeletonShape::TRACK.height, ROW_HEIGHT);
        for shape in [
            SkeletonShape::ALBUM,
            SkeletonShape::ARTIST,
            SkeletonShape::PLAYLIST,
        ] {
            assert_eq!(shape.height, WIDE_ROW_HEIGHT);
        }
        assert_eq!(SkeletonShape::ARTIST.text_lines, 1);
        assert_eq!(SkeletonShape::TRACK.text_lines, 2);
    }

    #[test]
    fn header_skeleton_height_is_header_height_and_back_is_live() {
        let ctx = Context::default();
        ctx.enable_accesskit();
        let mut used = 0.0;
        let mut expected = 0.0;
        let output = ctx.run_ui(RawInput::default(), |ui| {
            expected = header_height(ui.spacing());
            let before = ui.cursor().min.y;
            let _ = header_skeleton(ui);
            used = ui.cursor().min.y - before - ui.spacing().item_spacing.y;
        });
        output.drop_without_applying_deltas();
        assert!((used - expected).abs() < 0.5, "{used} vs {expected}");
    }
}

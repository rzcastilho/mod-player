// SPDX-License-Identifier: MIT OR Apache-2.0

//! The hover scrub indicator (022-waveform-legibility, US3, contracts/
//! ui-waveform-legibility.md WL5, data-model.md §8): a stateless 1px line
//! plus an `m:ss.mmm` label that follows the pointer on whichever waveform
//! view it hovers — recomputed fresh every frame, never stored, never
//! producing a [`super::WaveformEvent`] or touching AccessKit (FR-006-
//! FR-009, FR-016, FR-019).

use egui::{Align2, Painter, Pos2, Rect, Stroke, Vec2, Visuals, pos2};

use crate::theme;

use super::coords::TimeSpace;
use super::format_mmss_millis;

/// This frame's hover scrub indicator (data-model.md §8): built fresh by
/// [`hover_indicator`] every frame, never carried over from the last one.
#[derive(Debug, Clone, PartialEq)]
pub struct HoverIndicator {
    /// Pointer x, clamped to `space.rect`.
    pub x: f32,
    /// `space.frame_at(x)` — the frame under the pointer.
    pub frame: u64,
    /// `m:ss.mmm` label text at [`Self::frame`].
    pub text: String,
    /// The label pill's rect: placed right of the line by `space::XS`,
    /// flipped to the line's left on right-edge overflow, then clamped
    /// fully inside `space.rect`.
    pub label_rect: Rect,
}

/// This frame's [`HoverIndicator`], if any (contracts/
/// ui-waveform-legibility.md WL5): `None` while `suppressed` (a 005
/// seek-drag or 006 marker-drag in progress) or while `pointer` is `None`
/// — the caller passes `None` whenever the view is disabled or
/// `response.hover_pos()` itself is `None`, so this function never has to
/// re-derive "enabled" on its own. Pure: takes the label's own
/// already-measured `label_galley_size` (the mono `m:ss.mmm` text's own
/// size, no padding) rather than laying text out itself, so it needs no
/// `Ui`/fonts access and stays directly unit-testable; the padding around
/// that size is [`theme::space::XS`] on every side (WL5).
///
/// ```
/// # use egui::{Rect, Vec2, pos2};
/// # use modplayer_ui::waveform::{TimeSpace, hover_indicator};
/// let space = TimeSpace::new(
///     Rect::from_min_max(pos2(0.0, 0.0), pos2(200.0, 64.0)),
///     0..1_000,
///     44_100,
/// );
/// let indicator = hover_indicator(&space, Some(pos2(50.0, 10.0)), false, Vec2::new(60.0, 18.0))
///     .expect("a pointer is present and hover is not suppressed");
/// assert_eq!(indicator.x, 50.0);
/// assert!(space.rect.contains_rect(indicator.label_rect));
///
/// // Suppressed (a seek/marker drag in progress) or no pointer: `None`.
/// assert!(hover_indicator(&space, Some(pos2(50.0, 10.0)), true, Vec2::new(60.0, 18.0)).is_none());
/// assert!(hover_indicator(&space, None, false, Vec2::new(60.0, 18.0)).is_none());
/// ```
#[must_use]
pub fn hover_indicator(
    space: &TimeSpace,
    pointer: Option<Pos2>,
    suppressed: bool,
    label_galley_size: Vec2,
) -> Option<HoverIndicator> {
    if suppressed {
        return None;
    }
    let pointer = pointer?;
    let rect = space.rect;
    let x = pointer.x.clamp(rect.left(), rect.right());
    let frame = space.frame_at(x);
    let text = format_mmss_millis(frame, space.sample_rate);
    let label_size = Vec2::new(
        label_galley_size.x + 2.0 * theme::space::XS,
        label_galley_size.y + 2.0 * theme::space::XS,
    );
    let label_rect = label_rect_for(rect, x, label_size);
    Some(HoverIndicator {
        x,
        frame,
        text,
        label_rect,
    })
}

/// The label pill's rect (WL5): top-aligned at `rect.top() + space::XS`,
/// placed right of the line by `space::XS`; if that overflows
/// `rect.right()`, placed so its right edge sits `space::XS` left of the
/// line instead; finally translated (never resized) so it sits fully
/// inside `rect`.
fn label_rect_for(rect: Rect, x: f32, size: Vec2) -> Rect {
    let top = rect.top() + theme::space::XS;
    let right_placement = x + theme::space::XS;
    let left = if right_placement + size.x > rect.right() {
        x - theme::space::XS - size.x
    } else {
        right_placement
    };
    clamp_inside(Rect::from_min_size(pos2(left, top), size), rect)
}

/// `r` translated (its size is never changed) so it sits fully inside
/// `bounds` — valid wherever `r`'s own size already fits inside `bounds`,
/// true of the label pill at any of this feature's supported waveform
/// heights (data-model.md §8's proptest).
fn clamp_inside(r: Rect, bounds: Rect) -> Rect {
    let size = r.size();
    let max_left = (bounds.right() - size.x).max(bounds.left());
    let max_top = (bounds.bottom() - size.y).max(bounds.top());
    let left = r.left().clamp(bounds.left(), max_left);
    let top = r.top().clamp(bounds.top(), max_top);
    Rect::from_min_size(pos2(left, top), size)
}

/// Paint the hover line and label (WL5): a [`theme::waveform::
/// HOVER_LINE_WIDTH`] line in `hover_line`, full rect height, at
/// `indicator.x`; the `m:ss.mmm` label in [`theme::mono_font_id`],
/// `hover_label_text` on a `hover_label_bg` pill ([`theme::radius::SM`]),
/// centred in `indicator.label_rect`. Called between the `overlays` hook
/// and the playhead (WL1), so a marker/loop overlay paints under it and
/// the playhead always paints over it.
pub fn paint_hover(
    painter: &Painter,
    space: &TimeSpace,
    indicator: &HoverIndicator,
    visuals: &Visuals,
) {
    let rect = space.rect;
    let tokens = theme::waveform_roles(theme::roles(visuals));
    painter.line_segment(
        [
            pos2(indicator.x, rect.top()),
            pos2(indicator.x, rect.bottom()),
        ],
        Stroke::new(theme::waveform::HOVER_LINE_WIDTH, tokens.hover_line),
    );
    painter.rect_filled(
        indicator.label_rect,
        theme::radius::SM,
        tokens.hover_label_bg,
    );
    painter.text(
        indicator.label_rect.center(),
        Align2::CENTER_CENTER,
        &indicator.text,
        theme::mono_font_id(),
        tokens.hover_label_text,
    );
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;

    fn space(width: f32, height: f32) -> TimeSpace {
        TimeSpace::new(
            Rect::from_min_max(pos2(0.0, 0.0), pos2(width, height)),
            0..1_000,
            44_100,
        )
    }

    const LABEL_SIZE: Vec2 = Vec2::new(60.0, 18.0);

    #[test]
    fn none_when_suppressed() {
        let space = space(200.0, 64.0);
        assert!(hover_indicator(&space, Some(pos2(50.0, 10.0)), true, LABEL_SIZE).is_none());
    }

    #[test]
    fn none_when_no_pointer() {
        let space = space(200.0, 64.0);
        assert!(hover_indicator(&space, None, false, LABEL_SIZE).is_none());
    }

    #[test]
    fn follows_the_pointer_and_maps_to_the_frame() {
        let space = space(200.0, 64.0);
        let a = hover_indicator(&space, Some(pos2(20.0, 5.0)), false, LABEL_SIZE).unwrap();
        let b = hover_indicator(&space, Some(pos2(180.0, 5.0)), false, LABEL_SIZE).unwrap();
        assert_eq!(a.x, 20.0);
        assert_eq!(b.x, 180.0);
        assert_eq!(a.frame, space.frame_at(20.0));
        assert_eq!(b.frame, space.frame_at(180.0));
        assert!(a.frame < b.frame);
    }

    #[test]
    fn pointer_x_is_clamped_to_the_rect() {
        let space = space(200.0, 64.0);
        let left = hover_indicator(&space, Some(pos2(-500.0, 5.0)), false, LABEL_SIZE).unwrap();
        let right = hover_indicator(&space, Some(pos2(5_000.0, 5.0)), false, LABEL_SIZE).unwrap();
        assert_eq!(left.x, space.rect.left());
        assert_eq!(right.x, space.rect.right());
    }

    #[test]
    fn label_flips_left_on_right_edge_overflow() {
        // Wide enough that the flipped placement doesn't itself need the
        // final "clamp fully inside" step to move it again — isolates the
        // flip decision from the clamp.
        let space = space(300.0, 64.0);

        // Near the right edge: placing the label to the right of the line
        // would overflow `rect.right()`, so it must flip to the left.
        let indicator = hover_indicator(&space, Some(pos2(295.0, 5.0)), false, LABEL_SIZE).unwrap();
        assert!(
            indicator.label_rect.right() <= indicator.x,
            "label must sit to the line's left once flipped: {:?} vs x={}",
            indicator.label_rect,
            indicator.x
        );

        // Away from either edge: the normal (right-of-line) placement.
        let indicator = hover_indicator(&space, Some(pos2(150.0, 5.0)), false, LABEL_SIZE).unwrap();
        assert!(indicator.label_rect.left() >= indicator.x);
    }

    #[test]
    fn label_rect_is_fully_inside_the_rect_at_either_edge() {
        let space = space(200.0, 64.0);
        for x in [0.0, 1.0, 100.0, 199.0, 200.0] {
            let indicator = hover_indicator(&space, Some(pos2(x, 5.0)), false, LABEL_SIZE).unwrap();
            assert!(
                space.rect.contains_rect(indicator.label_rect),
                "x={x}: label_rect {:?} escapes rect {:?}",
                indicator.label_rect,
                space.rect
            );
        }
    }

    proptest::proptest! {
        /// data-model.md §8's validation: `label_rect ⊆ space.rect` for
        /// every pointer x, at both of this feature's supported minimum
        /// waveform heights (018-window-sizing, overview 64px / detail
        /// 120px), across a range of realistic widths.
        #[test]
        fn label_rect_always_inside_rect_at_minimum_heights(
            width in 150.0f32..4_000.0,
            height in proptest::prop_oneof![proptest::prelude::Just(64.0f32), proptest::prelude::Just(120.0f32)],
            x_fraction in 0.0f32..=1.0,
        ) {
            let space = space(width, height);
            let x = space.rect.left() + x_fraction * space.rect.width();
            let indicator = hover_indicator(&space, Some(pos2(x, 5.0)), false, LABEL_SIZE)
                .expect("pointer present, not suppressed");
            proptest::prop_assert!(
                space.rect.contains_rect(indicator.label_rect),
                "label_rect {:?} escapes rect {:?} (width={width}, height={height}, x={x})",
                indicator.label_rect,
                space.rect
            );
        }
    }
}

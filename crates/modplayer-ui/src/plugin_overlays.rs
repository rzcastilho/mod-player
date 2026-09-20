// SPDX-License-Identifier: MIT OR Apache-2.0

//! Waveform overlay painting (011-plugin-ui-contributions US3,
//! contracts/overlays-settings-notify.md §1.2 "O" rules): re-projects
//! every `Active` plugin's registered [`OverlayLayer`]s onto the current
//! [`TimeSpace`] every frame — the paint reads the registry's view fresh
//! each time, so zoom/scroll cost zero plugin calls (O10, SC-003). The
//! sole read model is `PlaybackController::plugin_overlays`; this module
//! never mutates anything and holds no state of its own. No colour
//! literal appears here (contracts/ui-panels.md A4): every colour comes
//! from `theme::overlay_color`/`theme::paint_host_glyph`.

use egui::{Align2, Color32, FontId, Painter, Rect, Stroke, Vec2, pos2};
use modplayer_capability_gateway::ui::{GlyphRef, HostGlyph, OverlayPrimitive};
use modplayer_core::plugins::OverlayLayer;

use crate::plugin_assets;
use crate::theme;
use crate::waveform::TimeSpace;

/// Which waveform widget is painting (O7: a `label` primitive draws only
/// in the detail view).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewKind {
    Overview,
    Detail,
}

/// O7: the glyph/label lane's height, reserved directly above the
/// waveform rect in both views — `now_playing.rs` adds this to each
/// view's own reserved height (`ui.add_space`, T089) *before* calling
/// `waveform::overview`/`detail`, so painting `LANE_HEIGHT` above
/// `space.rect`'s top edge lands inside that reserved blank strip rather
/// than over already-drawn content.
pub const LANE_HEIGHT: f32 = 16.0;
const GLYPH_SIZE: f32 = 12.0;
const LABEL_FONT_SIZE: f32 = 10.0;

/// O5-O10: paint every layer's primitives for one waveform widget's
/// current frame, in [`OverlayLayer`] order (already the correct
/// cross-plugin z-order, O4 — this function never re-sorts). `len_frames`
/// is the *whole track's* currently-known length, not `space`'s own
/// visible window (which, for the detail view, is a narrower sub-range):
/// O6's "beyond the streaming duration" skip needs the real total.
pub fn paint(
    painter: &Painter,
    space: &TimeSpace,
    len_frames: u64,
    view: ViewKind,
    layers: &[OverlayLayer],
) {
    let rect = space.rect;
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let ctx = painter.ctx();
    let visuals = ctx.style_of(ctx.theme()).visuals.clone();
    let lane_rect = Rect::from_min_max(
        pos2(rect.left(), rect.top() - LANE_HEIGHT),
        pos2(rect.right(), rect.top()),
    );

    for layer in layers {
        for primitive in &layer.primitives {
            paint_primitive(
                painter, space, len_frames, view, &visuals, rect, lane_rect, layer, primitive,
            );
        }
    }
}

/// O6: track-time milliseconds to a frame count, at `sample_rate`.
fn ms_to_frame(at_ms: u64, sample_rate: u32) -> u64 {
    at_ms.saturating_mul(u64::from(sample_rate)) / 1000
}

#[allow(clippy::too_many_arguments)]
fn paint_primitive(
    painter: &Painter,
    space: &TimeSpace,
    len_frames: u64,
    view: ViewKind,
    visuals: &egui::Visuals,
    rect: Rect,
    lane_rect: Rect,
    layer: &OverlayLayer,
    primitive: &OverlayPrimitive,
) {
    match primitive {
        OverlayPrimitive::Line { at_ms, color, .. } => {
            let frame = ms_to_frame(*at_ms, space.sample_rate);
            if frame > len_frames {
                return;
            }
            let x = space.x_of(frame);
            painter.line_segment(
                [pos2(x, rect.top()), pos2(x, rect.bottom())],
                Stroke::new(1.0, theme::overlay_color(*color, visuals)),
            );
        }
        OverlayPrimitive::Region {
            from_ms,
            to_ms,
            color,
            ..
        } => {
            let from_frame = ms_to_frame(*from_ms, space.sample_rate);
            if from_frame > len_frames {
                return;
            }
            let to_frame =
                ms_to_frame(*to_ms, space.sample_rate).min(len_frames.max(from_frame + 1));
            let x0 = space.x_of(from_frame);
            let x1 = space.x_of(to_frame).max(x0 + 1.0);
            let region_rect = Rect::from_min_max(pos2(x0, rect.top()), pos2(x1, rect.bottom()));
            painter.rect_filled(
                region_rect,
                0.0,
                theme::overlay_color(*color, visuals).gamma_multiply(0.25),
            );
        }
        OverlayPrimitive::Label {
            at_ms, text, color, ..
        } => {
            // O7: detail view only.
            if view != ViewKind::Detail {
                return;
            }
            let frame = ms_to_frame(*at_ms, space.sample_rate);
            if frame > len_frames {
                return;
            }
            let x = space.x_of(frame);
            // O7: "clipped to the view" — never bleeds past the waveform's
            // own left/right edges even though it paints in the lane
            // above it.
            let clip = Rect::from_min_max(
                pos2(rect.left(), lane_rect.top()),
                pos2(rect.right(), lane_rect.bottom()),
            );
            painter.with_clip_rect(clip).text(
                pos2(x, lane_rect.center().y),
                Align2::LEFT_CENTER,
                text,
                FontId::proportional(LABEL_FONT_SIZE),
                theme::overlay_color(*color, visuals),
            );
        }
        OverlayPrimitive::Glyph {
            at_ms, icon, color, ..
        } => {
            let frame = ms_to_frame(*at_ms, space.sample_rate);
            if frame > len_frames {
                return;
            }
            let x = space.x_of(frame);
            let center = pos2(x, lane_rect.center().y);
            let resolved = theme::overlay_color(*color, visuals);
            match icon {
                GlyphRef::Host(host) => {
                    theme::paint_host_glyph(painter, *host, center, GLYPH_SIZE, resolved)
                }
                GlyphRef::Package(key) => {
                    match plugin_assets::glyph_texture_id(
                        painter.ctx(),
                        layer.plugin,
                        key,
                        &layer.glyph_assets,
                    ) {
                        Some(texture_id) => {
                            let image_rect =
                                Rect::from_center_size(center, Vec2::splat(GLYPH_SIZE));
                            painter.image(
                                texture_id,
                                image_rect,
                                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                                Color32::WHITE,
                            );
                        }
                        // FR-014a: a `Package` key that never resolved
                        // (over-limit/missing/not yet embedded) falls back
                        // to the same generic glyph a header icon does —
                        // a neutral dot, not the requested colour token
                        // (signals "this is a fallback", `plugin_assets::
                        // generic_glyph`'s own behaviour).
                        None => theme::paint_host_glyph(
                            painter,
                            HostGlyph::Dot,
                            center,
                            GLYPH_SIZE,
                            visuals.weak_text_color(),
                        ),
                    }
                }
            }
        }
    }
}

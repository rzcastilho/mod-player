// SPDX-License-Identifier: MIT OR Apache-2.0

//! Host widgets for the control-variant grammar (015-control-variants,
//! data-model.md §7): `button`, `switch`, `row_frame`,
//! `destructive_gap` and the app-wide `paint_focus_ring` pass. These
//! consume the values defined in `theme::controls` and never write a
//! colour, alpha or stroke width of their own (FR-019).

use egui::accesskit::{Role, Toggled};
use egui::{
    Button, Color32, Context, Frame, Id, Label, LayerId, Order, Response, RichText, Sense,
    StrokeKind, TextWrapMode, Ui, WidgetInfo, WidgetType, pos2,
};

use crate::theme::{controls, tokens};

// ---------------------------------------------------------------------
// `button` (data-model.md §7, research R4/R6)
// ---------------------------------------------------------------------

/// Render `variant` as an `egui::Button`. State is resolved **before**
/// drawing, from last pass's `Response` for this widget's own id
/// (research R4) — `hovered()`/`is_pointer_button_down_on()`, **never**
/// `widget_state()` (I7, which folds focus into `Active`). The overlay is
/// composited into the fill handed to `egui::Button`, so the label is
/// never tinted (research R4, rejecting an over-paint). The label is
/// always built with `TextWrapMode::Extend` (018-window-sizing-and-
/// responsive-dock, contracts D4/D7): a plain `ui.horizontal` already
/// defaults to `Extend`, so this is a no-op there, but a caller that
/// draws this inside a `horizontal_wrapped` row (a wrapping transport row,
/// a header's overflow button row) would otherwise get `Wrap`, letting
/// egui shrink the label's text inside the button instead of moving the
/// whole button to the next line — never what this design system wants.
pub fn button(ui: &mut Ui, variant: controls::Variant, text: impl Into<RichText>) -> Response {
    let roles = tokens::roles(ui.visuals());
    let paint = controls::variant_paint(roles, variant);

    // `next_auto_id()` and `ui.add(...)` below must stay adjacent
    // (research R4/R7): nothing that allocates a widget id may run
    // between them, or the id read here is not the id the button takes.
    let id = ui.next_auto_id();
    let last_pass = ui.ctx().read_response(id);
    let hovered = last_pass.as_ref().is_some_and(Response::hovered);
    let pressed = last_pass
        .as_ref()
        .is_some_and(Response::is_pointer_button_down_on);

    let resting = if paint.fill == Color32::TRANSPARENT {
        roles.surface_base
    } else {
        paint.fill
    };
    let fill = if pressed {
        resting.blend(controls::pressed_fill(roles))
    } else if hovered {
        resting.blend(controls::hover_fill(roles))
    } else {
        paint.fill
    };

    ui.add(
        Button::new(text.into().color(paint.label))
            .fill(fill)
            .stroke(paint.outline)
            .wrap_mode(TextWrapMode::Extend),
    )
}

// ---------------------------------------------------------------------
// `switch` (data-model.md §5, §7–§8, research R1/R9)
// ---------------------------------------------------------------------

/// Which accessible role a [`switch`] reports (data-model.md §8, contract
/// A1): `Checkbox` for anything that is an `egui::Checkbox` today,
/// `Toggle` for anything that is a `selectable_label`/`toggle_value`
/// today (design note 9). Never uniform — `plugins_view.rs` counts and
/// asserts the absence of `Role::CheckBox` nodes in another state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitchKind {
    Checkbox,
    Toggle,
}

/// A pill-track switch with a moving thumb (data-model.md §5) — the host
/// widget every persistent boolean this feature converts renders through,
/// including the two lines that draw plugin-contributed booleans
/// (research R1). `kind` controls only the reported accessible role
/// (data-model.md §8); the visual is identical either way.
pub fn switch(ui: &mut Ui, kind: SwitchKind, on: &mut bool, label: &str) -> Response {
    let roles = tokens::roles(ui.visuals());
    let metrics = controls::switch_metrics();

    // A dedicated id, read fresh every pass (research R4/R7): `ui.horizontal`
    // below registers its own (non-focusable, `Sense::hover`) widget rect
    // for its child `Ui`'s id, so re-using `inner.response.id` for the
    // click-sensing `interact` below would double-register that same id —
    // one hover-only entry and one force-registered click entry — which
    // breaks egui's Tab focus-advance order past this control. A fresh id
    // avoids the collision entirely.
    let id = ui.next_auto_id();

    let inner = ui.horizontal(|ui| {
        let (track_rect, _reserved) = ui.allocate_exact_size(metrics.track, Sense::hover());
        if ui.is_rect_visible(track_rect) {
            let (track_fill, track_outline) = controls::switch_track(roles, *on);
            let corner = tokens::radius::full(metrics.track.y);
            ui.painter().rect(
                track_rect,
                corner,
                track_fill,
                track_outline,
                StrokeKind::Inside,
            );

            let thumb_left = if *on {
                track_rect.right() - metrics.inset - metrics.thumb
            } else {
                track_rect.left() + metrics.inset
            };
            let thumb_center = pos2(thumb_left + metrics.thumb / 2.0, track_rect.center().y);
            ui.painter().circle_filled(
                thumb_center,
                metrics.thumb / 2.0,
                controls::switch_thumb(roles, *on),
            );
        }
        ui.label(label);
    });

    let rect = inner.response.rect;
    // Host controls never read `widget_state()` (I7): the switch resolves
    // its own click from the `Response` it interacts for, this pass.
    let mut response = ui.interact(rect, id, Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }

    let widget_type = match kind {
        SwitchKind::Checkbox => WidgetType::Checkbox,
        SwitchKind::Toggle => WidgetType::SelectableLabel,
    };
    let enabled = ui.is_enabled();
    let value = *on;
    response.widget_info(|| WidgetInfo::selected(widget_type, enabled, value, label));

    response
}

// ---------------------------------------------------------------------
// `row_frame` (research R5)
// ---------------------------------------------------------------------

/// Reserve a shape index before `add`'s content runs, then `set` it to
/// the row's hover/pressed fill afterward (research R5) — the fill paints
/// **beneath** the content the caller already draws, at zero layout cost
/// (FR-018): no new container, no rect the caller didn't already own.
/// `id` drives the interaction the fill is read from — pass the row's own
/// id so hover state matches the row's existing click response.
pub fn row_frame<R>(ui: &mut Ui, id: Id, add: impl FnOnce(&mut Ui) -> R) -> (egui::Rect, R) {
    let where_to_put_background = ui.painter().add(egui::Shape::Noop);
    let inner = ui.scope(add);
    let rect = inner.response.rect;
    let response = ui.interact(rect, id, Sense::hover());

    if ui.is_rect_visible(rect) {
        let roles = tokens::roles(ui.visuals());
        let fill = if response.is_pointer_button_down_on() {
            Some(roles.surface_base.blend(controls::pressed_fill(roles)))
        } else if response.hovered() {
            Some(roles.surface_base.blend(controls::hover_fill(roles)))
        } else {
            None
        };
        if let Some(fill) = fill {
            ui.painter().set(
                where_to_put_background,
                egui::Shape::rect_filled(rect, egui::CornerRadius::ZERO, fill),
            );
        }
    }

    (rect, inner.inner)
}

// ---------------------------------------------------------------------
// `tab` (016-list-row-and-panel-components, data-model.md §9,
// contracts/tab-strip.md T1-T13)
// ---------------------------------------------------------------------

/// The Library tab strip's host widget (FR-013, FR-016, FR-034): draws
/// `label`, then strokes [`controls::tab_underline`] along the widget's
/// bottom edge when `selected` — a stroke, never a filled `accent`
/// background (T1). Sets `Role::Tab`, the exact `label` as accessible
/// name (T9 — no count suffix), and both `set_selected`/`set_toggled` on
/// the active tab (T11) — what `selectable_label` gave for free via
/// `WidgetInfo::selected` (`response.rs:976`), replaced by hand since
/// this is no longer a `selectable_label` (research R6).
pub fn tab(ui: &mut Ui, selected: bool, label: &str) -> Response {
    let roles = tokens::roles(ui.visuals());
    let response = ui.add(Label::new(label).sense(Sense::click()));

    if selected && ui.is_rect_visible(response.rect) {
        ui.painter().hline(
            response.rect.x_range(),
            response.rect.bottom(),
            controls::tab_underline(roles),
        );
    }

    ui.ctx().accesskit_node_builder(response.id, |b| {
        b.set_role(Role::Tab);
        b.set_label(label.to_string());
        b.set_selected(selected);
        b.set_toggled(if selected {
            Toggled::True
        } else {
            Toggled::False
        });
    });

    response
}

// ---------------------------------------------------------------------
// `panel_card` (016-list-row-and-panel-components, data-model.md §10,
// contracts/panel-card.md C1-C13)
// ---------------------------------------------------------------------

/// The one shared card every Now Playing block (Markers, Effect Chain,
/// Transport, Queue) renders through (FR-017, FR-036): `roles.
/// surface_raised` fill, `space::LG` padding on all four sides, `radius::
/// MD` corner radius, and no stroke (C2), topped by a `section`-role
/// uppercase header whose accessible name is pinned to the exact,
/// un-uppercased `header` string (FR-018, C3/C4 — mirrors `markers.rs`'s
/// own T030 pin, since `section_label` uppercases only the *painted*
/// text). `add_contents` draws the panel's own body inside the same card,
/// below the header.
pub fn panel_card(ui: &mut Ui, header: &str, add_contents: impl FnOnce(&mut Ui)) {
    let roles = tokens::roles(ui.visuals());
    Frame::new()
        .fill(roles.surface_raised)
        .inner_margin(tokens::space::LG)
        .corner_radius(tokens::radius::MD)
        .show(ui, |ui| {
            let heading = ui.label(tokens::section_label(header));
            ui.ctx().accesskit_node_builder(heading.id, |b| {
                b.set_role(Role::Heading);
                b.set_label(header.to_string());
            });
            add_contents(ui);
        });
}

// ---------------------------------------------------------------------
// `destructive_gap` (data-model.md §7, FR-006)
// ---------------------------------------------------------------------

/// `ui.add_space(DESTRUCTIVE_GAP)` — call before (or after) a
/// `Destructive` control that shares a row or control group with a
/// non-destructive one. Where the destructive control has no neighbour,
/// simply do not call this (Edge Cases, design note 13).
pub fn destructive_gap(ui: &mut Ui) {
    ui.add_space(controls::DESTRUCTIVE_GAP);
}

// ---------------------------------------------------------------------
// `paint_focus_ring` (research R2/R3, contract F1–F5)
// ---------------------------------------------------------------------

/// The **one** app-level focus-ring pass (research R3, contract F4): call
/// as the last statement of `App::ui`. Reads `Memory::focused()` and
/// `Context::read_response`, strokes one ring into a foreground layer, and
/// returns silently when nothing is focused or the focused widget was not
/// visible this pass or last (F5) — no view file paints a ring.
pub fn paint_focus_ring(ctx: &Context) {
    let Some(id) = ctx.memory(|m| m.focused()) else {
        return;
    };
    let Some(response) = ctx.read_response(id) else {
        return;
    };

    let dark_mode = matches!(ctx.theme(), egui::Theme::Dark);
    let roles = tokens::for_dark_mode(dark_mode);
    let ring = controls::focus_ring(roles);

    let painter = ctx.layer_painter(LayerId::new(Order::Foreground, Id::new("focus-ring")));
    painter.rect_stroke(
        response.rect.expand(controls::FOCUS_RING_GAP),
        tokens::radius::SM,
        ring,
        StrokeKind::Outside,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::accesskit::{Role, Toggled};
    use egui::{Event, Modifiers, PointerButton, RawInput, Rect};

    /// T011 (contract I7): `button()` resolves hover/pressed from the
    /// `Response`'s own fields — never `widget_state()` — so its
    /// interaction state must track a real hover/press across passes
    /// exactly like any other egui widget (research R4's "simulates two
    /// passes to prove the state tracks").
    #[test]
    fn button_hover_and_press_track_across_passes() {
        let ctx = Context::default();
        crate::theme::apply_tokens(&ctx);

        // Pass 1: establish the button's rect.
        let mut rect = Rect::NOTHING;
        let output = ctx.run_ui(RawInput::default(), |ui| {
            rect = button(ui, controls::Variant::Default, "Test").rect;
        });
        output.drop_without_applying_deltas();

        // Pass 2: the pointer hovers over pass 1's rect.
        let hover_pos = rect.center();
        let mut hovered = false;
        let output = ctx.run_ui(
            RawInput {
                events: vec![Event::PointerMoved(hover_pos)],
                ..Default::default()
            },
            |ui| {
                hovered = button(ui, controls::Variant::Default, "Test").hovered();
            },
        );
        output.drop_without_applying_deltas();
        assert!(
            hovered,
            "expected hover to be detected via Response fields on pass 2"
        );

        // Pass 3: the primary button is held down over the same rect.
        let mut pressed = false;
        let output = ctx.run_ui(
            RawInput {
                events: vec![
                    Event::PointerMoved(hover_pos),
                    Event::PointerButton {
                        pos: hover_pos,
                        button: PointerButton::Primary,
                        pressed: true,
                        modifiers: Modifiers::NONE,
                    },
                ],
                ..Default::default()
            },
            |ui| {
                pressed =
                    button(ui, controls::Variant::Default, "Test").is_pointer_button_down_on();
            },
        );
        output.drop_without_applying_deltas();
        assert!(
            pressed,
            "expected is_pointer_button_down_on() to be detected via Response fields on pass 3"
        );
    }

    /// T011 (contract A1): `SwitchKind::Checkbox` reports `Role::CheckBox`
    /// with the current `on` value as `Toggled`.
    #[test]
    fn switch_checkbox_kind_reports_checkbox_role_and_toggled() {
        let ctx = Context::default();
        crate::theme::apply_tokens(&ctx);
        ctx.enable_accesskit();

        let mut on = true;
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            switch(ui, SwitchKind::Checkbox, &mut on, "Test switch");
        });
        let update = output
            .platform_output
            .accesskit_update
            .take()
            .unwrap_or_else(|| unreachable!("accesskit_update should be populated once enabled"));
        output.drop_without_applying_deltas();

        let found = update.nodes.iter().any(|(_, node)| {
            node.role() == Role::CheckBox && node.toggled() == Some(Toggled::True)
        });
        assert!(found, "expected a Role::CheckBox node toggled true");
    }

    /// T011 (contract A1): `SwitchKind::Toggle` reports `Role::Button`
    /// (`WidgetType::SelectableLabel`'s mapping) with the current `on`
    /// value as `Toggled`.
    #[test]
    fn switch_toggle_kind_reports_button_role_and_toggled() {
        let ctx = Context::default();
        crate::theme::apply_tokens(&ctx);
        ctx.enable_accesskit();

        let mut on = false;
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            switch(ui, SwitchKind::Toggle, &mut on, "Test toggle");
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
            .any(|(_, node)| node.role() == Role::Button && node.toggled() == Some(Toggled::False));
        assert!(found, "expected a Role::Button node toggled false");
    }

    /// C2 (contracts/panel-card.md): `panel_card`'s frame fills with
    /// `roles.surface_raised`, insets `space::LG` on every side (checked
    /// against the header's own accesskit bounds — the first thing drawn
    /// inside the frame's content area), uses `radius::MD` corner radius,
    /// and draws no stroke — in both themes.
    #[test]
    fn panel_card_uses_surface_raised_lg_padding_md_radius_no_stroke() {
        for dark_mode in [false, true] {
            let ctx = Context::default();
            crate::theme::apply_tokens(&ctx);
            ctx.set_theme(if dark_mode {
                egui::Theme::Dark
            } else {
                egui::Theme::Light
            });
            ctx.enable_accesskit();
            let roles = tokens::for_dark_mode(dark_mode);

            let mut output = ctx.run_ui(RawInput::default(), |ui| {
                panel_card(ui, "Panel Header", |ui| {
                    ui.label("body");
                });
            });
            let update = output
                .platform_output
                .accesskit_update
                .take()
                .unwrap_or_else(|| {
                    unreachable!("accesskit_update should be populated once enabled")
                });

            let bg = output
                .shapes
                .iter()
                .find_map(|clipped| match &clipped.shape {
                    egui::Shape::Rect(rect_shape) if rect_shape.fill == roles.surface_raised => {
                        Some(rect_shape.clone())
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("expected a surface_raised background rect"));
            assert_eq!(
                bg.corner_radius,
                tokens::radius::MD,
                "panel_card must use radius::MD"
            );
            assert_eq!(
                bg.stroke.width, 0.0,
                "panel_card must draw no stroke, got {:?}",
                bg.stroke
            );

            let heading_bounds = update
                .nodes
                .iter()
                .find(|(_, node)| node.role() == Role::Heading)
                .and_then(|(_, node)| node.bounds())
                .unwrap_or_else(|| panic!("expected a Role::Heading node with bounds"));
            assert!(
                (heading_bounds.x0 as f32 - bg.rect.left() - tokens::space::LG).abs() < 0.5,
                "left padding must be space::LG: heading x0={}, bg left={}",
                heading_bounds.x0,
                bg.rect.left()
            );
            assert!(
                (heading_bounds.y0 as f32 - bg.rect.top() - tokens::space::LG).abs() < 0.5,
                "top padding must be space::LG: heading y0={}, bg top={}",
                heading_bounds.y0,
                bg.rect.top()
            );

            output.drop_without_applying_deltas();
        }
    }

    /// C3/C4 (contracts/panel-card.md): the header renders through
    /// `theme::section_label` (uppercase, `section` role) but its
    /// accessible name stays the exact, un-uppercased string.
    #[test]
    fn panel_card_header_is_uppercase_section_role_with_exact_accessible_name() {
        let ctx = Context::default();
        crate::theme::apply_tokens(&ctx);
        ctx.enable_accesskit();

        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            panel_card(ui, "queue", |_ui| {});
        });
        let uppercase_painted = output.shapes.iter().any(|clipped| {
            matches!(&clipped.shape, egui::Shape::Text(t) if t.galley.job.text == "QUEUE")
        });
        let update = output
            .platform_output
            .accesskit_update
            .take()
            .unwrap_or_else(|| unreachable!("accesskit_update should be populated once enabled"));
        output.drop_without_applying_deltas();

        assert!(
            uppercase_painted,
            "the header must be painted uppercase via theme::section_label"
        );
        let heading = update
            .nodes
            .iter()
            .find(|(_, node)| node.role() == Role::Heading)
            .unwrap_or_else(|| panic!("expected a Role::Heading node"));
        assert_eq!(
            heading.1.label(),
            Some("queue"),
            "the accessible name must be the exact, un-uppercased header string"
        );
    }
}

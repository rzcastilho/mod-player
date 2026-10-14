// SPDX-License-Identifier: MIT OR Apache-2.0

//! The shared status message (030-onboarding-clarity, contracts/
//! onboarding-screens.md §S1): one framed row with a leading warning bar,
//! used for every sign-in reason so the cause of a state is always drawn
//! the same way, between the heading and the actions.

use egui::accesskit::{Live, Role};
use egui::{Frame, Margin, Response, Sense, Stroke, Ui, Vec2};

use crate::theme::{self, tokens};

/// Draw `text` as a status message: a framed row (divider stroke,
/// `radius::SM`) with a leading `space::XS`-wide bar in the `warning` role
/// and the text in the body style, capped at the prose measure. The
/// AccessKit node is a polite live `Role::Status` named by `text`.
///
/// ```no_run
/// # fn demo(ui: &mut egui::Ui) {
/// let response = modplayer_ui::widgets::status::status_message(ui, "Sign-in was cancelled.");
/// # let _ = response;
/// # }
/// ```
pub fn status_message(ui: &mut Ui, text: &str) -> Response {
    let roles = tokens::roles(ui.visuals());
    let divider = theme::divider_color(ui.visuals());
    let measure = theme::body_measure(ui.ctx());
    let width = ui.available_width().min(measure);

    let outer = ui
        .scope(|ui| {
            ui.set_max_width(width);
            let inner = Frame::new()
                .stroke(Stroke::new(1.0, divider))
                .corner_radius(theme::radius::SM)
                .inner_margin(Margin::same(theme::space::SM as i8))
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        let (bar, _) = ui.allocate_exact_size(
                            Vec2::new(theme::space::XS, ui.text_style_height(&theme::text::BODY)),
                            Sense::hover(),
                        );
                        let text_response = ui.label(text);
                        let bar = egui::Rect::from_min_max(
                            bar.min,
                            egui::pos2(bar.max.x, text_response.rect.max.y.max(bar.max.y)),
                        );
                        ui.painter().rect_filled(bar, 0.0, roles.warning);
                        text_response
                    })
                    .inner
                });
            inner.inner
        })
        .inner;

    // The label is the live status node; give it the role and name.
    let name = text.to_owned();
    ui.ctx().accesskit_node_builder(outer.id, |builder| {
        builder.set_role(Role::Status);
        builder.set_live(Live::Polite);
        builder.set_label(name);
    });
    outer
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::disallowed_methods)]
mod tests {
    use super::*;

    #[test]
    fn paints_a_warning_bar_and_one_status_node() {
        let ctx = egui::Context::default();
        theme::apply_tokens(&ctx);
        ctx.enable_accesskit();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(960.0, 640.0),
            )),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            status_message(ui, "Sign-in was cancelled.");
        });
        let update = output.platform_output.accesskit_update.take().expect("ak");
        let statuses: Vec<_> = update
            .nodes
            .iter()
            .filter(|(_, n)| n.role() == Role::Status)
            .collect();
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].1.label(), Some("Sign-in was cancelled."));
        assert_eq!(statuses[0].1.live(), Some(Live::Polite));

        let warning = tokens::roles(&ctx.global_style().visuals).warning;
        let mut bars = 0;
        for clipped in &output.shapes {
            if let egui::Shape::Rect(r) = &clipped.shape
                && r.fill == warning
            {
                bars += 1;
            }
        }
        assert_eq!(bars, 1, "one warning-coloured bar");
        output.drop_without_applying_deltas();
    }
}

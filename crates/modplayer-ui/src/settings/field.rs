// SPDX-License-Identifier: MIT OR Apache-2.0

//! The shared settings-field builder (028-settings-fields-and-account,
//! research R1/R2, data-model §3.1–3.2): one row = label, indented help,
//! the control line (control plus an optional per-field Reset), and an
//! optional range caption. Every Settings category screen draws its fields
//! through [`row`] so spacing, help indent and Reset placement are uniform.
//!
//! The pure helpers ([`format_number`], [`format_value`], [`parse_value`],
//! [`ResetState::compute`]) carry no egui state and are unit-tested here.

use egui::accesskit::Role;
use egui::{Rect, Response, RichText, Ui};
use modplayer_core::{tr, tr_args};

use crate::theme::controls::Variant;
use crate::theme::tokens::{self, space, text};
use crate::widgets::controls::button;

/// The unit a numeric setting is displayed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    /// Decibels full scale; shown with one decimal.
    Dbfs,
    /// Percent; shown as an integer.
    Percent,
    /// Milliseconds; shown as an integer.
    Ms,
}

impl Unit {
    fn value_key(self) -> &'static str {
        match self {
            Unit::Dbfs => "setting-value-dbfs",
            Unit::Percent => "setting-value-percent",
            Unit::Ms => "setting-value-ms",
        }
    }

    fn range_key(self) -> &'static str {
        match self {
            Unit::Dbfs => "setting-range-dbfs",
            Unit::Percent => "setting-range-percent",
            Unit::Ms => "setting-range-ms",
        }
    }

    /// Suffixes [`parse_value`] strips, longest first, lower-case.
    fn suffixes(self) -> &'static [&'static str] {
        match self {
            Unit::Dbfs => &["dbfs", "db"],
            Unit::Percent => &["%"],
            Unit::Ms => &["ms"],
        }
    }
}

/// Remove Fluent's bidi-isolation marks around placeables.
pub(crate) fn strip_isolates(s: String) -> String {
    s.replace(['\u{2068}', '\u{2069}'], "")
}

/// The bare number for `unit`: one decimal for dBFS, an integer otherwise.
///
/// # Examples
///
/// ```
/// use modplayer_ui::settings::field::{Unit, format_number};
/// assert_eq!(format_number(Unit::Dbfs, -1.0), "-1.0");
/// assert_eq!(format_number(Unit::Percent, 50.0), "50");
/// assert_eq!(format_number(Unit::Ms, 10.0), "10");
/// ```
pub fn format_number(unit: Unit, value: f64) -> String {
    match unit {
        Unit::Dbfs => format!("{value:.1}"),
        Unit::Percent | Unit::Ms => format!("{value:.0}"),
    }
}

/// The value with its unit, localized ("-1.0 dBFS", "50%", "10 ms").
///
/// # Examples
///
/// ```
/// use modplayer_ui::settings::field::{Unit, format_value};
/// assert_eq!(format_value(Unit::Dbfs, -1.0), "-1.0 dBFS");
/// assert_eq!(format_value(Unit::Percent, 50.0), "50%");
/// ```
pub fn format_value(unit: Unit, value: f64) -> String {
    strip_isolates(tr_args(
        unit.value_key(),
        &[("value", format_number(unit, value))],
    ))
}

/// The range caption for `unit` ("-6.0 to -0.1 dBFS").
///
/// # Examples
///
/// ```
/// use modplayer_ui::settings::field::{Unit, format_range};
/// assert_eq!(format_range(Unit::Percent, 0.0, 100.0), "0 to 100%");
/// ```
pub fn format_range(unit: Unit, min: f64, max: f64) -> String {
    strip_isolates(tr_args(
        unit.range_key(),
        &[
            ("min", format_number(unit, min)),
            ("max", format_number(unit, max)),
        ],
    ))
}

/// Parse a number typed into a slider/drag field, with or without the
/// unit. `None` for anything else.
///
/// # Examples
///
/// ```
/// use modplayer_ui::settings::field::{Unit, parse_value};
/// assert_eq!(parse_value(Unit::Dbfs, "-3.5 dBFS"), Some(-3.5));
/// assert_eq!(parse_value(Unit::Ms, "20"), Some(20.0));
/// assert_eq!(parse_value(Unit::Percent, "abc"), None);
/// ```
pub fn parse_value(unit: Unit, text: &str) -> Option<f64> {
    let lower = text.trim().to_lowercase();
    let mut number = lower.as_str();
    for suffix in unit.suffixes() {
        if let Some(stripped) = number.strip_suffix(suffix) {
            number = stripped;
            break;
        }
    }
    number.trim().parse::<f64>().ok().filter(|v| v.is_finite())
}

/// Whether a field's Reset control is offered (data-model §3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetState {
    /// No Reset control is drawn.
    Hidden,
    /// Current differs from default and a write path exists.
    Offered,
}

impl ResetState {
    /// `Offered` only when `current` differs from `default`.
    ///
    /// # Examples
    ///
    /// ```
    /// use modplayer_ui::settings::field::ResetState;
    /// assert_eq!(ResetState::compute(&10, &10), ResetState::Hidden);
    /// assert_eq!(ResetState::compute(&20, &10), ResetState::Offered);
    /// ```
    pub fn compute<T: PartialEq>(current: &T, default: &T) -> Self {
        if current == default {
            ResetState::Hidden
        } else {
            ResetState::Offered
        }
    }
}

/// The eight fields that offer a per-field Reset (data-model §3.3). Each
/// compares its current value with `AudioSettings::default()` and writes the
/// default back through the same setter its control uses. Output device,
/// language, "Test output device", Account, Controls, Developer and plugin
/// fields are deliberately absent.
pub const RESETTABLE_IDS: [&str; 8] = [
    "audio.buffer_preset",
    "audio.limiter_ceiling",
    "audio.safe_volume_enabled",
    "audio.safe_volume_cap",
    "appearance.theme",
    crate::settings::appearance::HIGH_CONTRAST_FIELD_ID,
    "playback.device_name",
    "markers.nudge_step_ms",
];

/// Whether `id` is one of the [`RESETTABLE_IDS`].
///
/// # Examples
///
/// ```
/// use modplayer_ui::settings::field::is_resettable;
/// assert!(is_resettable("audio.limiter_ceiling"));
/// assert!(!is_resettable("language.locale"));
/// ```
pub fn is_resettable(id: &str) -> bool {
    RESETTABLE_IDS.contains(&id)
}

/// Memory key for the focus deferred past a Reset (data-model §3.5's
/// `pending_focus`, held in egui's temp memory so every category screen
/// shares it without a signature change).
fn pending_focus_key() -> egui::Id {
    egui::Id::new("settings-field-pending-focus")
}

/// Memory key for the descriptor id of the active search-result
/// highlight, published by `SettingsScreen` each frame.
fn highlight_target_key() -> egui::Id {
    egui::Id::new("settings-field-highlight-target")
}

/// Memory key for the rect the highlight was last painted at.
fn highlight_painted_key() -> egui::Id {
    egui::Id::new("settings-field-highlight-painted")
}

/// Publish (or clear) the descriptor id being highlighted this frame and
/// whether the highlight is already armed (past its selecting frame).
pub(crate) fn set_highlight_target(ctx: &egui::Context, target: Option<(&'static str, bool)>) {
    ctx.data_mut(|d| {
        d.remove::<Rect>(highlight_painted_key());
        match target {
            Some(target) => {
                d.insert_temp::<(&'static str, bool)>(highlight_target_key(), target);
            }
            None => d.remove::<(&'static str, bool)>(highlight_target_key()),
        }
    });
}

/// `Some(armed)` when `id` is the active highlight target.
pub(crate) fn highlight_state(ctx: &egui::Context, id: &str) -> Option<bool> {
    match ctx.data(|d| d.get_temp::<(&'static str, bool)>(highlight_target_key())) {
        Some((target, armed)) if target == id => Some(armed),
        _ => None,
    }
}

/// The rect the search highlight was painted at during the most recent
/// frame, or `None` when that frame painted no highlight.
#[must_use]
pub fn painted_highlight(ctx: &egui::Context) -> Option<Rect> {
    ctx.data(|d| d.get_temp::<Rect>(highlight_painted_key()))
}

/// Paint the search-result highlight around `rect` (stroke at least 2 px in
/// the accent role plus a translucent accent fill) and, on the frame it is
/// first shown (`scroll`), scroll `rect` into view.
pub(crate) fn paint_highlight(ui: &mut Ui, rect: Rect, scroll: bool) {
    let roles = tokens::roles(ui.visuals());
    let width = crate::theme::controls::focus_ring_width(roles).max(2.0);
    // Clamp to the clip rect so a field flush with the viewport edge is
    // still fully outlined inside it.
    let outline = rect.expand(space::XS).intersect(ui.clip_rect());
    let fill = roles.accent.gamma_multiply(0.12);
    ui.painter().rect(
        outline,
        crate::theme::tokens::radius::SM,
        fill,
        egui::Stroke::new(width, roles.accent),
        egui::StrokeKind::Outside,
    );
    ui.ctx()
        .data_mut(|d| d.insert_temp::<Rect>(highlight_painted_key(), outline));
    if scroll {
        ui.scroll_to_rect(outline, Some(egui::Align::Center));
    }
}

/// One field's inputs for [`row`] (data-model §3.1).
pub struct FieldSpec {
    /// Descriptor id (`"audio.limiter_ceiling"`…).
    pub id: &'static str,
    /// Localized label; also the Reset accessible-name `{field}`.
    pub label: String,
    /// Localized help text, drawn indented.
    pub help: Option<String>,
    /// Pre-formatted range caption.
    pub range: Option<String>,
    /// Whether Reset is offered.
    pub reset: ResetState,
    /// This field is the active search-highlight target.
    pub highlight: bool,
    /// The control paints `label` itself (a switch), so [`row`] does not
    /// draw it a second time. `label` still names the Reset control.
    pub label_in_control: bool,
}

impl FieldSpec {
    /// A spec with only `id` and `label`; everything else off.
    pub fn new(id: &'static str, label: String) -> Self {
        Self {
            id,
            label,
            help: None,
            range: None,
            reset: ResetState::Hidden,
            highlight: false,
            label_in_control: false,
        }
    }
}

/// What [`row`] reports back (data-model §3.2).
pub struct FieldOutput {
    /// The whole field: label, help, control line and caption.
    pub rect: Rect,
    /// The control's own response.
    pub control: Response,
    /// The label's widget id (for `labelled_by`), when [`row`] drew one.
    pub label_id: Option<egui::Id>,
    /// Reset was activated this frame (click, Enter or Space).
    pub reset_clicked: bool,
}

/// Draw one settings field: label → help → control line (control, then
/// Reset when offered) → range caption.
pub fn row(
    ui: &mut Ui,
    spec: &FieldSpec,
    add_control: impl FnOnce(&mut Ui) -> Response,
) -> FieldOutput {
    let roles = tokens::roles(ui.visuals());
    let mut control: Option<Response> = None;
    let mut reset_clicked = false;
    let mut label_id = None;
    // A Reset on the previous frame asked this field's control to take
    // focus (the Reset button it was on has just disappeared).
    let take_focus = ui.ctx().data_mut(|d| {
        let key = pending_focus_key();
        if d.get_temp::<&'static str>(key) == Some(spec.id) {
            d.remove::<&'static str>(key);
            true
        } else {
            false
        }
    });
    let scope = ui.scope(|ui| {
        if !spec.label_in_control {
            label_id = Some(ui.label(&spec.label).id);
        }
        if let Some(help) = &spec.help {
            ui.horizontal_top(|ui| {
                ui.add_space(space::SM);
                ui.scope(|ui| {
                    ui.set_max_width(ui.available_width().min(tokens::body_measure(ui.ctx())));
                    // Inside a horizontal layout labels extend instead of
                    // wrapping; help must wrap at the measure.
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                    ui.label(
                        RichText::new(help)
                            .text_style(text::SECONDARY)
                            .color(roles.text_secondary),
                    );
                });
            });
        }
        ui.horizontal(|ui| {
            // Fields sit on a `panel_card` (`surface_raised`), the colour
            // egui paints slider rails and knobs with; sink them to
            // `surface_base` so the rail stays visible (manual walk M1).
            // Buttons and combo boxes fill from `weak_bg_fill`, unaffected.
            let response = ui
                .scope(|ui| {
                    ui.visuals_mut().widgets.inactive.bg_fill = roles.surface_base;
                    add_control(ui)
                })
                .inner;
            if take_focus {
                response.request_focus();
            }
            control = Some(response);
            if spec.reset == ResetState::Offered {
                let reset = button(ui, Variant::Default, tr("settings-reset"));
                let name = strip_isolates(tr_args(
                    "settings-reset-a11y",
                    &[("field", spec.label.clone())],
                ));
                ui.ctx().accesskit_node_builder(reset.id, |b| {
                    b.set_role(Role::Button);
                    b.set_label(name);
                });
                reset_clicked = reset.clicked();
                if reset_clicked {
                    let id = spec.id;
                    ui.ctx()
                        .data_mut(|d| d.insert_temp::<&'static str>(pending_focus_key(), id));
                    ui.ctx().request_repaint();
                }
            }
        });
        if let Some(range) = &spec.range {
            ui.label(
                RichText::new(range)
                    .text_style(text::SECONDARY)
                    .color(roles.text_secondary),
            );
        }
    });
    let highlight = highlight_state(ui.ctx(), spec.id);
    if spec.highlight || highlight.is_some() {
        paint_highlight(ui, scope.response.rect, highlight == Some(false));
    }
    FieldOutput {
        rect: scope.response.rect,
        control: control.unwrap_or(scope.response),
        label_id,
        reset_clicked,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_formatting_per_unit() {
        assert_eq!(format_number(Unit::Dbfs, -1.0), "-1.0");
        assert_eq!(format_number(Unit::Dbfs, -0.15), "-0.1");
        assert_eq!(format_number(Unit::Percent, 50.0), "50");
        assert_eq!(format_number(Unit::Ms, 10.0), "10");
    }

    #[test]
    fn value_and_range_captions_include_unit() {
        assert_eq!(format_value(Unit::Dbfs, -1.0), "-1.0 dBFS");
        assert_eq!(format_value(Unit::Percent, 50.0), "50%");
        assert_eq!(format_value(Unit::Ms, 10.0), "10 ms");
        assert_eq!(format_range(Unit::Dbfs, -6.0, -0.1), "-6.0 to -0.1 dBFS");
        assert_eq!(format_range(Unit::Ms, 1.0, 1000.0), "1 to 1000 ms");
    }

    #[test]
    fn parser_accepts_number_with_or_without_unit() {
        assert_eq!(parse_value(Unit::Dbfs, "-3.5"), Some(-3.5));
        assert_eq!(parse_value(Unit::Dbfs, " -3.5 dBFS "), Some(-3.5));
        assert_eq!(parse_value(Unit::Percent, "40%"), Some(40.0));
        assert_eq!(parse_value(Unit::Ms, "20 ms"), Some(20.0));
        assert_eq!(parse_value(Unit::Ms, "20"), Some(20.0));
    }

    #[test]
    fn parser_rejects_garbage() {
        assert_eq!(parse_value(Unit::Dbfs, "loud"), None);
        assert_eq!(parse_value(Unit::Ms, ""), None);
        assert_eq!(parse_value(Unit::Percent, "NaN"), None);
        assert_eq!(parse_value(Unit::Ms, "ms"), None);
    }

    #[test]
    fn exactly_eight_resettable_fields_all_searchable() {
        assert_eq!(RESETTABLE_IDS.len(), 8);
        for id in RESETTABLE_IDS {
            assert!(
                modplayer_core::settings_registry::DESCRIPTORS
                    .iter()
                    .any(|d| d.id == id),
                "{id} must be a registered descriptor"
            );
        }
        assert!(!is_resettable("audio.output_device"));
        assert!(!is_resettable("language.locale"));
    }

    /// Manual walk M1 regression: inside a `panel_card` the card fill is
    /// `surface_raised`, the same colour egui paints a slider rail with
    /// (`widgets.inactive.bg_fill`), so the rail vanished. The control must
    /// see a rail fill that differs from the card fill, in every theme.
    #[test]
    fn slider_rail_is_visible_on_the_card_fill() {
        for hc in [false, true] {
            for theme in [egui::Theme::Light, egui::Theme::Dark] {
                let ctx = egui::Context::default();
                crate::theme::apply_tokens_for(&ctx, hc);
                ctx.set_theme(theme);
                let mut seen = None;
                let mut card = None;
                let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                    card = Some(tokens::roles(ui.visuals()).surface_raised);
                    crate::widgets::controls::panel_card(ui, "Group", |ui| {
                        let spec = FieldSpec::new("audio.limiter_ceiling", "Ceiling".into());
                        row(ui, &spec, |ui| {
                            seen = Some(ui.visuals().widgets.inactive.bg_fill);
                            ui.label("control")
                        });
                    });
                });
                output.drop_without_applying_deltas();
                assert_ne!(seen, card, "{theme:?} hc={hc}: rail fill equals card fill");
            }
        }
    }

    #[test]
    fn reset_offered_only_when_changed() {
        assert_eq!(ResetState::compute(&-1.0_f32, &-1.0), ResetState::Hidden);
        assert_eq!(ResetState::compute(&-2.0_f32, &-1.0), ResetState::Offered);
        assert_eq!(ResetState::compute(&true, &true), ResetState::Hidden);
    }
}

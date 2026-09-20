// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `ui.*` namespace's closed vocabularies and pure validators
//! (011-plugin-ui-contributions, data-model.md §1.3, contracts/
//! plugin-api-v1.2.md, research R3). Every type here is plain data; the
//! *stateful* registries (which plugin owns which panel/overlay/action/
//! schema) live in `modplayer-core::plugins::ui` (Constitution III) —
//! this crate only checks that a call's shape is well-formed before core
//! ever touches its own state.

pub mod limits;

use std::collections::BTreeSet;

use crate::refusal::Refusal;

/// A validated panel/widget/overlay/action/settings-field identifier
/// (FR-002a): `[a-z][a-z0-9_]{0,63}` ([`limits::ID_GRAMMAR`]).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UiId(String);

impl UiId {
    /// `None` if `s` fails [`limits::ID_GRAMMAR`].
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        if limits::matches_id_grammar(s) {
            Some(Self(s.to_string()))
        } else {
            None
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for UiId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A widget's kind (contract §3.1). The order here is the order the
/// contract's Lua examples list them in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetKind {
    Label,
    Button,
    Toggle,
    Slider,
    Knob,
    List,
    Text,
    MarkerList,
    Meter,
}

impl WidgetKind {
    /// The wire string a plugin's `kind = "..."` field uses.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "label" => WidgetKind::Label,
            "button" => WidgetKind::Button,
            "toggle" => WidgetKind::Toggle,
            "slider" => WidgetKind::Slider,
            "knob" => WidgetKind::Knob,
            "list" => WidgetKind::List,
            "text" => WidgetKind::Text,
            "marker_list" => WidgetKind::MarkerList,
            "meter" => WidgetKind::Meter,
            _ => return None,
        })
    }

    #[must_use]
    pub const fn wire_name(self) -> &'static str {
        match self {
            WidgetKind::Label => "label",
            WidgetKind::Button => "button",
            WidgetKind::Toggle => "toggle",
            WidgetKind::Slider => "slider",
            WidgetKind::Knob => "knob",
            WidgetKind::List => "list",
            WidgetKind::Text => "text",
            WidgetKind::MarkerList => "marker_list",
            WidgetKind::Meter => "meter",
        }
    }
}

/// One `list`/`choice` entry (contract §3.1/§3.5).
#[derive(Debug, Clone, PartialEq)]
pub struct ListItem {
    pub id: UiId,
    pub label: String,
}

/// One widget declaration inside `register_panel` (contract §3.1,
/// data-model.md §1.3). Every field beyond `id`/`kind`/`label` is only
/// meaningful for some kinds — [`validate_panel`] enforces which.
#[derive(Debug, Clone, PartialEq)]
pub struct WidgetSpec {
    pub id: UiId,
    pub kind: WidgetKind,
    /// Resolved (R17, `@key` substitution) before this reaches
    /// validation; an empty label is `unlabeled_widget`.
    pub label: String,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Option<f64>,
    pub value: Option<f64>,
    pub items: Vec<ListItem>,
    pub selected: Option<UiId>,
    /// `button` only: this plugin's own action name (FR-008); a target
    /// action id not otherwise validated here (unknown targets are
    /// resolved, and refused if unknown, at invocation time in core).
    pub action: Option<String>,
    /// `label`/`text` initial content.
    pub text: Option<String>,
}

/// A committed widget value (contract §3.2, `panel_interaction` payload).
#[derive(Debug, Clone, PartialEq)]
pub enum WidgetValue {
    Bool(bool),
    Number(f64),
    Text(String),
    Item(UiId),
    Items {
        items: Vec<ListItem>,
        selected: Option<UiId>,
    },
}

/// An overlay primitive's colour token (contract §3.3): `theme.rs` (UI
/// crate) is the only place these map to an actual `Color32`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OverlayColor {
    #[default]
    Accent,
    Secondary,
    Positive,
    Warning,
    Neutral,
}

impl OverlayColor {
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "accent" => OverlayColor::Accent,
            "secondary" => OverlayColor::Secondary,
            "positive" => OverlayColor::Positive,
            "warning" => OverlayColor::Warning,
            "neutral" => OverlayColor::Neutral,
            _ => return None,
        })
    }
}

/// A host-drawn glyph name (contract §3.3) usable without a manifest
/// asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostGlyph {
    Dot,
    Flag,
    Note,
    Chord,
    Star,
    Warning,
}

impl HostGlyph {
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "dot" => HostGlyph::Dot,
            "flag" => HostGlyph::Flag,
            "note" => HostGlyph::Note,
            "chord" => HostGlyph::Chord,
            "star" => HostGlyph::Star,
            "warning" => HostGlyph::Warning,
            _ => return None,
        })
    }
}

/// Where an overlay `glyph` primitive's icon comes from (contract §3.3):
/// one of the closed [`HostGlyph`] set, or a key into the manifest's own
/// `[glyphs]` table (checked against the caller-supplied `glyph_keys`
/// set by [`validate_primitives`] — the gateway crate has no manifest
/// access of its own).
#[derive(Debug, Clone, PartialEq)]
pub enum GlyphRef {
    Host(HostGlyph),
    Package(String),
}

/// One overlay primitive (contract §3.3, data-model.md §1.3). Positions
/// are track-time milliseconds.
#[derive(Debug, Clone, PartialEq)]
pub enum OverlayPrimitive {
    Line {
        id: UiId,
        at_ms: u64,
        color: OverlayColor,
    },
    Region {
        id: UiId,
        from_ms: u64,
        to_ms: u64,
        color: OverlayColor,
    },
    Label {
        id: UiId,
        at_ms: u64,
        text: String,
        color: OverlayColor,
    },
    Glyph {
        id: UiId,
        at_ms: u64,
        icon: GlyphRef,
        color: OverlayColor,
    },
}

impl OverlayPrimitive {
    #[must_use]
    pub fn id(&self) -> &UiId {
        match self {
            OverlayPrimitive::Line { id, .. }
            | OverlayPrimitive::Region { id, .. }
            | OverlayPrimitive::Label { id, .. }
            | OverlayPrimitive::Glyph { id, .. } => id,
        }
    }
}

/// A shortcut action's dispatch shape (contract §3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionKindSpec {
    Trigger,
    Continuous,
}

impl ActionKindSpec {
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "trigger" => ActionKindSpec::Trigger,
            "continuous" => ActionKindSpec::Continuous,
            _ => return None,
        })
    }
}

/// One `register_action` declaration (contract §3.4).
#[derive(Debug, Clone, PartialEq)]
pub struct ActionSpec {
    pub id: UiId,
    pub label: String,
    pub kind: ActionKindSpec,
    /// 007's platform-neutral chord encoding, unparsed here (a bad/
    /// unparseable binding registers the action unbound with a console
    /// warning — never a refusal, R6/research).
    pub default_binding: Option<String>,
    pub repeats_while_held: bool,
}

/// A settings field's type and, for `number`/`choice`, its own bounds
/// (contract §3.5).
#[derive(Debug, Clone, PartialEq)]
pub enum FieldKind {
    Boolean,
    Number { min: f64, max: f64, step: f64 },
    String,
    Choice { options: Vec<ListItem> },
}

/// One `register_settings` field declaration (contract §3.5).
#[derive(Debug, Clone, PartialEq)]
pub struct SettingsField {
    pub id: UiId,
    pub kind: FieldKind,
    pub label: String,
    pub description: Option<String>,
    pub default: serde_json::Value,
}

/// A notification's severity (contract §3.6), 1:1 with `modplayer-core`'s
/// own `notifications::Severity`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyLevel {
    Critical,
    Warning,
    Info,
}

impl NotifyLevel {
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "critical" => NotifyLevel::Critical,
            "warning" => NotifyLevel::Warning,
            "info" => NotifyLevel::Info,
            _ => return None,
        })
    }
}

// -- Validators (pure; data-model.md §1.3) ----------------------------------
//
// Every validator returns the first failure only, ordered by widget/field
// index; messages name `widgets[<i>]`/`fields[<i>]` and append ` (<id>)`
// once an id has parsed (FR-024's fixed refusal vocabulary).

fn invalid(reason: &'static str, message: impl Into<String>) -> Refusal {
    Refusal::invalid_state(reason, message)
}

/// `register_panel(title, widgets)` (contract §3.1).
pub fn validate_panel(title: &str, widgets: &[WidgetSpec]) -> Result<(), Refusal> {
    if title.chars().count() > limits::MAX_PANEL_TITLE_CHARS {
        return Err(invalid(
            "invalid_value",
            format!(
                "title exceeds {} characters.",
                limits::MAX_PANEL_TITLE_CHARS
            ),
        ));
    }
    if widgets.len() > limits::MAX_WIDGETS_PER_PANEL {
        return Err(invalid(
            "panel_widget_limit",
            format!(
                "A panel may declare at most {} widgets.",
                limits::MAX_WIDGETS_PER_PANEL
            ),
        ));
    }
    let mut seen_ids: BTreeSet<&str> = BTreeSet::new();
    for (i, widget) in widgets.iter().enumerate() {
        let path = format!("widgets[{i}]");
        if !seen_ids.insert(widget.id.as_str()) {
            return Err(invalid(
                "invalid_widget_id",
                format!("{path} ({}): duplicate widget id.", widget.id),
            ));
        }
        if widget.label.chars().count() > limits::MAX_LABEL_CHARS {
            return Err(invalid(
                "invalid_value",
                format!("{path} ({}): label exceeds 256 characters.", widget.id),
            ));
        }
        if widget.label.trim().is_empty() {
            return Err(invalid(
                "unlabeled_widget",
                format!("{path} ({}): widget has no label.", widget.id),
            ));
        }
        match widget.kind {
            WidgetKind::Slider | WidgetKind::Knob => {
                let (Some(min), Some(max)) = (widget.min, widget.max) else {
                    return Err(invalid(
                        "invalid_value",
                        format!("{path} ({}): min/max are required.", widget.id),
                    ));
                };
                if min.partial_cmp(&max) != Some(std::cmp::Ordering::Less) {
                    return Err(invalid(
                        "invalid_value",
                        format!("{path} ({}): min must be less than max.", widget.id),
                    ));
                }
                if let Some(step) = widget.step
                    && step.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater)
                {
                    return Err(invalid(
                        "invalid_value",
                        format!("{path} ({}): step must be positive.", widget.id),
                    ));
                }
                if let Some(value) = widget.value
                    && !(min..=max).contains(&value)
                {
                    return Err(invalid(
                        "invalid_value",
                        format!("{path} ({}): value is out of range.", widget.id),
                    ));
                }
            }
            WidgetKind::List => {
                if widget.items.len() > limits::MAX_LIST_ITEMS {
                    return Err(invalid(
                        "invalid_value",
                        format!(
                            "{path} ({}): more than {} items.",
                            widget.id,
                            limits::MAX_LIST_ITEMS
                        ),
                    ));
                }
                let mut item_ids: BTreeSet<&str> = BTreeSet::new();
                for item in &widget.items {
                    if !item_ids.insert(item.id.as_str()) {
                        return Err(invalid(
                            "invalid_value",
                            format!("{path} ({}): duplicate item id.", widget.id),
                        ));
                    }
                }
                if let Some(selected) = &widget.selected
                    && !widget.items.iter().any(|i| &i.id == selected)
                {
                    return Err(invalid(
                        "invalid_value",
                        format!("{path} ({}): selected item is unknown.", widget.id),
                    ));
                }
            }
            WidgetKind::Text | WidgetKind::Label => {
                if let Some(text) = &widget.text
                    && text.chars().count() > limits::MAX_TEXT_CHARS
                {
                    return Err(invalid(
                        "invalid_value",
                        format!(
                            "{path} ({}): text exceeds {} characters.",
                            widget.id,
                            limits::MAX_TEXT_CHARS
                        ),
                    ));
                }
            }
            WidgetKind::Button
            | WidgetKind::Toggle
            | WidgetKind::MarkerList
            | WidgetKind::Meter => {}
        }
    }
    Ok(())
}

/// `update_widget(panel, widget, value)`'s per-kind value check (contract
/// §3.2). Capacity (`not_found`) is the caller's job — this only checks
/// that `value` is the right shape for `spec.kind`.
pub fn validate_update(spec: &WidgetSpec, value: &WidgetValue) -> Result<(), Refusal> {
    match (spec.kind, value) {
        (WidgetKind::Toggle, WidgetValue::Bool(_)) => Ok(()),
        (WidgetKind::Slider | WidgetKind::Knob, WidgetValue::Number(n)) => {
            let (min, max) = (spec.min.unwrap_or(f64::MIN), spec.max.unwrap_or(f64::MAX));
            if (min..=max).contains(n) {
                Ok(())
            } else {
                Err(invalid("invalid_value", "value is out of range."))
            }
        }
        (WidgetKind::List, WidgetValue::Item(id)) => {
            if spec.items.iter().any(|i| &i.id == id) {
                Ok(())
            } else {
                Err(Refusal::not_found())
            }
        }
        (WidgetKind::List, WidgetValue::Items { items, selected }) => {
            if items.len() > limits::MAX_LIST_ITEMS {
                return Err(invalid(
                    "invalid_value",
                    format!("more than {} items.", limits::MAX_LIST_ITEMS),
                ));
            }
            if let Some(selected) = selected
                && !items.iter().any(|i| &i.id == selected)
            {
                return Err(Refusal::not_found());
            }
            Ok(())
        }
        (WidgetKind::Text, WidgetValue::Text(s)) => {
            if s.chars().count() > limits::MAX_TEXT_CHARS {
                Err(invalid(
                    "invalid_value",
                    format!("text exceeds {} characters.", limits::MAX_TEXT_CHARS),
                ))
            } else {
                Ok(())
            }
        }
        (WidgetKind::Meter, WidgetValue::Number(_)) => Ok(()), // clamped by core, never refused
        (WidgetKind::Label | WidgetKind::Button | WidgetKind::MarkerList, _) => Err(invalid(
            "not_updatable",
            "This widget kind cannot be updated.",
        )),
        _ => Err(invalid(
            "invalid_value",
            "The value's shape does not match this widget's kind.",
        )),
    }
}

/// `add_overlays(primitives)` (contract §3.3). `glyph_keys` is the
/// calling plugin's manifest `[glyphs]` key set (empty if it declared
/// none) — the gateway crate has no manifest of its own to consult.
pub fn validate_primitives(
    primitives: &[OverlayPrimitive],
    glyph_keys: &BTreeSet<String>,
) -> Result<(), Refusal> {
    for (i, primitive) in primitives.iter().enumerate() {
        let path = format!("primitives[{i}]");
        let id = primitive.id();
        match primitive {
            OverlayPrimitive::Region { from_ms, to_ms, .. } => {
                if from_ms >= to_ms {
                    return Err(invalid(
                        "invalid_value",
                        format!("{path} ({id}): from must be less than to."),
                    ));
                }
            }
            OverlayPrimitive::Label { text, .. } => {
                if text.chars().count() > limits::MAX_OVERLAY_LABEL_CHARS {
                    return Err(invalid(
                        "invalid_value",
                        format!(
                            "{path} ({id}): label exceeds {} characters.",
                            limits::MAX_OVERLAY_LABEL_CHARS
                        ),
                    ));
                }
            }
            OverlayPrimitive::Glyph { icon, .. } => {
                if let GlyphRef::Package(key) = icon
                    && !glyph_keys.contains(key)
                {
                    return Err(invalid(
                        "invalid_value",
                        format!("{path} ({id}): unknown glyph key."),
                    ));
                }
            }
            OverlayPrimitive::Line { .. } => {}
        }
    }
    Ok(())
}

/// `register_action(spec)` (contract §3.4). The default binding string
/// itself is never validated here (R6): an unparseable/rejected binding
/// registers the action unbound with a console warning, not a refusal.
pub fn validate_action(spec: &ActionSpec) -> Result<(), Refusal> {
    if spec.label.trim().is_empty() || spec.label.chars().count() > limits::MAX_LABEL_CHARS {
        return Err(invalid(
            "invalid_value",
            format!("({}): label must be 1-256 characters.", spec.id),
        ));
    }
    Ok(())
}

/// `register_settings(fields)` (contract §3.5).
pub fn validate_schema(fields: &[SettingsField]) -> Result<(), Refusal> {
    if fields.len() > limits::MAX_SETTINGS_FIELDS {
        return Err(invalid(
            "settings_field_limit",
            format!(
                "A settings schema may declare at most {} fields.",
                limits::MAX_SETTINGS_FIELDS
            ),
        ));
    }
    let mut seen_ids: BTreeSet<&str> = BTreeSet::new();
    for (i, field) in fields.iter().enumerate() {
        let path = format!("fields[{i}]");
        if !seen_ids.insert(field.id.as_str()) {
            return Err(invalid(
                "invalid_schema",
                format!("{path} ({}): duplicate field id.", field.id),
            ));
        }
        if field.label.trim().is_empty() {
            return Err(invalid(
                "invalid_schema",
                format!("{path} ({}): label must not be empty.", field.id),
            ));
        }
        match &field.kind {
            FieldKind::Boolean => {
                if !field.default.is_boolean() {
                    return Err(invalid(
                        "invalid_schema",
                        format!("{path} ({}): default must be a boolean.", field.id),
                    ));
                }
            }
            FieldKind::Number { min, max, step } => {
                if step.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
                    return Err(invalid(
                        "invalid_schema",
                        format!("{path} ({}): step must be positive.", field.id),
                    ));
                }
                let Some(default) = field.default.as_f64() else {
                    return Err(invalid(
                        "invalid_schema",
                        format!("{path} ({}): default must be a number.", field.id),
                    ));
                };
                if !(*min <= default && default <= *max) {
                    return Err(invalid(
                        "invalid_schema",
                        format!("{path} ({}): default is out of range.", field.id),
                    ));
                }
            }
            FieldKind::String => {
                let Some(default) = field.default.as_str() else {
                    return Err(invalid(
                        "invalid_schema",
                        format!("{path} ({}): default must be a string.", field.id),
                    ));
                };
                if default.chars().count() > limits::MAX_STRING_FIELD_CHARS {
                    return Err(invalid(
                        "invalid_schema",
                        format!("{path} ({}): default exceeds 1,024 characters.", field.id),
                    ));
                }
            }
            FieldKind::Choice { options } => {
                if options.is_empty() || options.len() > limits::MAX_CHOICE_OPTIONS {
                    return Err(invalid(
                        "invalid_schema",
                        format!("{path} ({}): 1-64 options are required.", field.id),
                    ));
                }
                let Some(default) = field.default.as_str() else {
                    return Err(invalid(
                        "invalid_schema",
                        format!("{path} ({}): default must be an option id.", field.id),
                    ));
                };
                if !options.iter().any(|o| o.id.as_str() == default) {
                    return Err(invalid(
                        "invalid_schema",
                        format!("{path} ({}): default is not one of the options.", field.id),
                    ));
                }
            }
        }
    }
    Ok(())
}

/// `notify(level, text)` (contract §3.6).
pub fn validate_notify(level_str: &str, text: &str) -> Result<(), Refusal> {
    if NotifyLevel::parse(level_str).is_none() {
        return Err(invalid(
            "invalid_value",
            "level must be critical, warning or info.",
        ));
    }
    if text.chars().count() > limits::MAX_NOTIFY_TEXT_CHARS {
        return Err(invalid(
            "invalid_value",
            format!("text exceeds {} characters.", limits::MAX_NOTIFY_TEXT_CHARS),
        ));
    }
    Ok(())
}

/// `get_settings()`'s default-substitution check (research R4): whether a
/// stored JSON value is still valid for `field`'s current kind/range —
/// an invalid or type-mismatched value substitutes `field.default`
/// instead (with a console warning; storage is never rewritten by a
/// read).
#[must_use]
pub fn validate_stored(field: &SettingsField, value: &serde_json::Value) -> bool {
    match &field.kind {
        FieldKind::Boolean => value.is_boolean(),
        FieldKind::Number { min, max, .. } => {
            value.as_f64().is_some_and(|v| (*min..=*max).contains(&v))
        }
        FieldKind::String => value
            .as_str()
            .is_some_and(|s| s.chars().count() <= limits::MAX_STRING_FIELD_CHARS),
        FieldKind::Choice { options } => value
            .as_str()
            .is_some_and(|s| options.iter().any(|o| o.id.as_str() == s)),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;

    fn label_widget(id: &str, label: &str) -> WidgetSpec {
        WidgetSpec {
            id: UiId::parse(id).unwrap(),
            kind: WidgetKind::Label,
            label: label.to_string(),
            min: None,
            max: None,
            step: None,
            value: None,
            items: Vec::new(),
            selected: None,
            action: None,
            text: None,
        }
    }

    #[test]
    fn empty_label_is_unlabeled_widget() {
        let err = validate_panel("Panel", &[label_widget("hdr", "")]).unwrap_err();
        assert_eq!(err.reason, "unlabeled_widget");
        assert!(err.message.contains("widgets[0]"));
        assert!(err.message.contains("(hdr)"));
    }

    #[test]
    fn valid_panel_passes() {
        assert!(validate_panel("Panel", &[label_widget("hdr", "Section")]).is_ok());
    }

    #[test]
    fn too_many_widgets_is_refused() {
        let widgets: Vec<WidgetSpec> = (0..101)
            .map(|i| label_widget(&format!("w{i}"), "Label"))
            .collect();
        let err = validate_panel("Panel", &widgets).unwrap_err();
        assert_eq!(err.reason, "panel_widget_limit");
    }

    #[test]
    fn duplicate_widget_ids_are_refused() {
        let err =
            validate_panel("Panel", &[label_widget("a", "A"), label_widget("a", "B")]).unwrap_err();
        assert_eq!(err.reason, "invalid_widget_id");
    }
}

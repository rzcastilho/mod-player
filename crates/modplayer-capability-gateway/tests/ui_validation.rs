// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! `ui.rs`'s pure validators (Constitution VIII, contracts/plugin-api-v1.2.md
//! §7): every refusal reason FR-024 lists, plus a proptest over the id
//! grammar.

use std::collections::BTreeSet;

use modplayer_capability_gateway::ui::{
    ActionKindSpec, ActionSpec, FieldKind, GlyphRef, HostGlyph, ListItem, OverlayColor,
    OverlayPrimitive, SettingsField, UiId, WidgetKind, WidgetSpec, WidgetValue, validate_action,
    validate_notify, validate_panel, validate_primitives, validate_schema, validate_update,
};
use proptest::prelude::*;
use serde_json::json;

fn widget(id: &str, kind: WidgetKind, label: &str) -> WidgetSpec {
    WidgetSpec {
        id: UiId::parse(id).expect("valid id"),
        kind,
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
fn unlabeled_widget_names_path() {
    let err = validate_panel("Panel", &[widget("tempo", WidgetKind::Label, "")]).unwrap_err();
    assert_eq!(err.reason, "unlabeled_widget");
    assert_eq!(err.message, "widgets[0] (tempo): widget has no label.");
}

#[test]
fn duplicate_widget_id() {
    let a = widget("a", WidgetKind::Label, "A");
    let b = widget("a", WidgetKind::Label, "B");
    let err = validate_panel("Panel", &[a, b]).unwrap_err();
    assert_eq!(err.reason, "invalid_widget_id");
}

#[test]
fn panel_widget_limit_101() {
    let widgets: Vec<WidgetSpec> = (0..101)
        .map(|i| widget(&format!("w{i}"), WidgetKind::Label, "Label"))
        .collect();
    let err = validate_panel("Panel", &widgets).unwrap_err();
    assert_eq!(err.reason, "panel_widget_limit");
}

#[test]
fn slider_bounds() {
    let mut slider = widget("tempo", WidgetKind::Slider, "Tempo");
    slider.min = Some(200.0);
    slider.max = Some(60.0); // min >= max
    let err = validate_panel("Panel", std::slice::from_ref(&slider)).unwrap_err();
    assert_eq!(err.reason, "invalid_value");

    slider.min = Some(60.0);
    slider.max = Some(200.0);
    slider.step = Some(0.0); // step must be > 0
    let err = validate_panel("Panel", std::slice::from_ref(&slider)).unwrap_err();
    assert_eq!(err.reason, "invalid_value");

    slider.step = Some(1.0);
    slider.value = Some(500.0); // out of [min, max]
    let err = validate_panel("Panel", &[slider]).unwrap_err();
    assert_eq!(err.reason, "invalid_value");
}

#[test]
fn list_item_bounds() {
    let mut list = widget("mode", WidgetKind::List, "Mode");
    list.items = (0..257)
        .map(|i| ListItem {
            id: UiId::parse(&format!("i{i}")).expect("valid"),
            label: "Item".to_string(),
        })
        .collect();
    let err = validate_panel("Panel", std::slice::from_ref(&list)).unwrap_err();
    assert_eq!(err.reason, "invalid_value");

    list.items = vec![ListItem {
        id: UiId::parse("a").expect("valid"),
        label: "A".to_string(),
    }];
    list.selected = Some(UiId::parse("unknown").expect("valid"));
    let err = validate_panel("Panel", &[list]).unwrap_err();
    assert_eq!(err.reason, "invalid_value");
}

#[test]
fn update_widget_kind_mismatches() {
    let toggle = widget("snap", WidgetKind::Toggle, "Snap");
    assert!(validate_update(&toggle, &WidgetValue::Bool(true)).is_ok());
    let err = validate_update(&toggle, &WidgetValue::Number(1.0)).unwrap_err();
    assert_eq!(err.reason, "invalid_value");

    let label = widget("hdr", WidgetKind::Label, "Header");
    let err = validate_update(&label, &WidgetValue::Text("x".to_string())).unwrap_err();
    assert_eq!(err.reason, "not_updatable");
}

#[test]
fn overlay_region_order() {
    let region = OverlayPrimitive::Region {
        id: UiId::parse("v1").expect("valid"),
        from_ms: 24_000,
        to_ms: 12_000,
        color: OverlayColor::Accent,
    };
    let err = validate_primitives(&[region], &BTreeSet::new()).unwrap_err();
    assert_eq!(err.reason, "invalid_value");
}

#[test]
fn overlay_unknown_icon() {
    let glyph = OverlayPrimitive::Glyph {
        id: UiId::parse("g1").expect("valid"),
        at_ms: 1_000,
        icon: GlyphRef::Package("missing".to_string()),
        color: OverlayColor::Accent,
    };
    let err = validate_primitives(&[glyph], &BTreeSet::new()).unwrap_err();
    assert_eq!(err.reason, "invalid_value");

    let mut known = BTreeSet::new();
    known.insert("ok".to_string());
    let glyph_ok = OverlayPrimitive::Glyph {
        id: UiId::parse("g1").expect("valid"),
        at_ms: 1_000,
        icon: GlyphRef::Package("ok".to_string()),
        color: OverlayColor::Accent,
    };
    assert!(validate_primitives(&[glyph_ok], &known).is_ok());

    let host_glyph = OverlayPrimitive::Glyph {
        id: UiId::parse("g2").expect("valid"),
        at_ms: 1_000,
        icon: GlyphRef::Host(HostGlyph::Chord),
        color: OverlayColor::Accent,
    };
    assert!(validate_primitives(&[host_glyph], &BTreeSet::new()).is_ok());
}

#[test]
fn schema_default_out_of_range() {
    let field = SettingsField {
        id: UiId::parse("shift").expect("valid"),
        kind: FieldKind::Number {
            min: -12.0,
            max: 12.0,
            step: 1.0,
        },
        label: "Semitone shift".to_string(),
        description: None,
        default: json!(50.0),
    };
    let err = validate_schema(&[field]).unwrap_err();
    assert_eq!(err.reason, "invalid_schema");
}

#[test]
fn notify_text_201() {
    let text = "x".repeat(201);
    let err = validate_notify("info", &text).unwrap_err();
    assert_eq!(err.reason, "invalid_value");
    assert!(validate_notify("info", "ok").is_ok());
    assert!(validate_notify("not_a_level", "ok").is_err());
}

#[test]
fn action_spec_requires_a_label() {
    let spec = ActionSpec {
        id: UiId::parse("take_over").expect("valid"),
        label: String::new(),
        kind: ActionKindSpec::Trigger,
        default_binding: Some("L".to_string()),
        repeats_while_held: false,
    };
    let err = validate_action(&spec).unwrap_err();
    assert_eq!(err.reason, "invalid_value");
}

proptest! {
    #[test]
    fn id_grammar_proptest(s in "[a-z][a-z0-9_]{0,63}") {
        prop_assert!(UiId::parse(&s).is_some());
    }

    #[test]
    fn id_grammar_rejects_uppercase_and_overlong(s in "[A-Z][a-z0-9_]{0,63}") {
        prop_assert!(UiId::parse(&s).is_none());
    }
}

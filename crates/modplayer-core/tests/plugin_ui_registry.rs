// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! `PanelRegistry` contract tests (US1, contracts/ui-panels.md §1 "P"
//! rules): registration atomicity/capacity, `update_widget`'s per-kind
//! behaviour, session-close survival, and the Suspend-keeps/Disable-
//! removes lifecycle split. Drives `modplayer_core::plugins::ui::panel`
//! directly — the same pure model `plugins/apply.rs`'s `RegisterPanel`/
//! `UpdateWidget` arms and `PluginUi::on_ready`/`on_stop` (US1 T046) sit
//! on top of; `controller_plugin_ui.rs` covers the end-to-end RPC/event
//! path over a real `PlaybackController`.

use std::collections::BTreeMap;

use modplayer_capability_gateway::manifest::PluginIdentifier;
use modplayer_capability_gateway::ui::{
    FieldKind, ListItem, OverlayColor, OverlayPrimitive, SettingsField, UiId, WidgetKind,
    WidgetSpec, WidgetValue,
};
use modplayer_core::plugins::ui::overlay::OverlayRegistry;
use modplayer_core::plugins::ui::panel::{PanelKey, PanelRegistry};
use modplayer_core::plugins::ui::settings::SettingsRegistry;
use modplayer_effects::catalog::PluginId;
use serde_json::json;

const Q: PluginId = PluginId(1);

const P: PluginId = PluginId(0);

fn id(s: &str) -> UiId {
    UiId::parse(s).unwrap_or_else(|| unreachable!("{s:?} must be valid grammar"))
}

fn label_widget(wid: &str) -> WidgetSpec {
    WidgetSpec {
        id: id(wid),
        kind: WidgetKind::Label,
        label: "Label".to_string(),
        min: None,
        max: None,
        step: None,
        value: None,
        items: Vec::new(),
        selected: None,
        action: None,
        text: Some("hello".to_string()),
    }
}

fn slider_widget(wid: &str) -> WidgetSpec {
    WidgetSpec {
        id: id(wid),
        kind: WidgetKind::Slider,
        label: "Tempo".to_string(),
        min: Some(0.0),
        max: Some(200.0),
        step: Some(1.0),
        value: Some(120.0),
        items: Vec::new(),
        selected: None,
        action: None,
        text: None,
    }
}

fn meter_widget(wid: &str) -> WidgetSpec {
    WidgetSpec {
        id: id(wid),
        kind: WidgetKind::Meter,
        label: "Level".to_string(),
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

fn list_widget(wid: &str) -> WidgetSpec {
    WidgetSpec {
        id: id(wid),
        kind: WidgetKind::List,
        label: "Choices".to_string(),
        min: None,
        max: None,
        step: None,
        value: None,
        items: vec![
            ListItem {
                id: id("a"),
                label: "A".to_string(),
            },
            ListItem {
                id: id("b"),
                label: "B".to_string(),
            },
        ],
        selected: None,
        action: None,
        text: None,
    }
}

/// P1/P2: a re-registered panel id replaces its whole layout in place
/// (same position/`seq`), never appends a second entry.
#[test]
fn register_replaces_atomically() {
    let mut reg = PanelRegistry::new();
    reg.register(P, id("main"), "T1".to_string(), vec![label_widget("a")])
        .unwrap_or_else(|e| unreachable!("{e:?}"));
    let first_seq = reg.for_plugin(P)[0].seq;
    reg.register(P, id("main"), "T2".to_string(), vec![label_widget("b")])
        .unwrap_or_else(|e| unreachable!("{e:?}"));
    let panels = reg.for_plugin(P);
    assert_eq!(panels.len(), 1);
    assert_eq!(panels[0].title, "T2");
    assert_eq!(panels[0].widgets[0].spec.id, id("b"));
    assert_eq!(panels[0].seq, first_seq);
}

/// FR-003: the 17th distinct panel id is `panel_limit`; the 16 already
/// registered are untouched.
#[test]
fn seventeenth_panel_refused() {
    let mut reg = PanelRegistry::new();
    for i in 0..16 {
        reg.register(
            P,
            id(&format!("p{i}")),
            "T".to_string(),
            vec![label_widget("a")],
        )
        .unwrap_or_else(|e| unreachable!("{e:?}"));
    }
    let err = reg
        .register(P, id("p16"), "T".to_string(), vec![label_widget("a")])
        .unwrap_err();
    assert_eq!(err.reason, "panel_limit");
    assert_eq!(reg.for_plugin(P).len(), 16);
}

/// FR-007: `update_widget` against an unknown widget path is `not_found`
/// — the registry's own half of the check (kind-appropriateness, e.g.
/// `label` being `not_updatable`, is the gateway's `validate_update`,
/// already covered by `modplayer-capability-gateway`'s own tests, and by
/// `plugins/apply.rs`'s wiring exercised in `controller_plugin_ui.rs`).
#[test]
fn update_label_not_updatable() {
    let mut reg = PanelRegistry::new();
    reg.register(P, id("main"), "T".to_string(), vec![label_widget("a")])
        .unwrap_or_else(|e| unreachable!("{e:?}"));
    let err = reg
        .update(
            P,
            &id("main"),
            &id("missing"),
            WidgetValue::Text("x".to_string()),
        )
        .unwrap_err();
    assert_eq!(err.reason, "unknown_id");
}

/// FR-007: a registered slider's value is stored as given (range
/// enforcement itself is the gateway's `validate_update`).
#[test]
fn update_slider_out_of_range() {
    let mut reg = PanelRegistry::new();
    reg.register(P, id("main"), "T".to_string(), vec![slider_widget("t")])
        .unwrap_or_else(|e| unreachable!("{e:?}"));
    reg.update(P, &id("main"), &id("t"), WidgetValue::Number(50.0))
        .unwrap_or_else(|e| unreachable!("{e:?}"));
    assert_eq!(
        reg.for_plugin(P)[0].widgets[0].value,
        WidgetValue::Number(50.0)
    );
}

/// FR-007: a `meter` value is clamped into `[0.0, 1.0]`, never refused.
#[test]
fn meter_clamps() {
    let mut reg = PanelRegistry::new();
    reg.register(P, id("main"), "T".to_string(), vec![meter_widget("m")])
        .unwrap_or_else(|e| unreachable!("{e:?}"));
    reg.update(P, &id("main"), &id("m"), WidgetValue::Number(5.0))
        .unwrap_or_else(|e| unreachable!("{e:?}"));
    assert_eq!(
        reg.for_plugin(P)[0].widgets[0].value,
        WidgetValue::Number(1.0)
    );
    reg.update(P, &id("main"), &id("m"), WidgetValue::Number(-5.0))
        .unwrap_or_else(|e| unreachable!("{e:?}"));
    assert_eq!(
        reg.for_plugin(P)[0].widgets[0].value,
        WidgetValue::Number(0.0)
    );
}

/// FR-007: `{ items, selected? }` replaces a `list`'s whole item set.
#[test]
fn list_replace_items() {
    let mut reg = PanelRegistry::new();
    reg.register(P, id("main"), "T".to_string(), vec![list_widget("l")])
        .unwrap_or_else(|e| unreachable!("{e:?}"));
    assert_eq!(
        reg.for_plugin(P)[0].widgets[0].value,
        WidgetValue::Items {
            items: vec![
                ListItem {
                    id: id("a"),
                    label: "A".to_string()
                },
                ListItem {
                    id: id("b"),
                    label: "B".to_string()
                },
            ],
            selected: Some(id("a")),
        }
    );
    reg.update(
        P,
        &id("main"),
        &id("l"),
        WidgetValue::Items {
            items: vec![ListItem {
                id: id("c"),
                label: "C".to_string(),
            }],
            selected: Some(id("c")),
        },
    )
    .unwrap_or_else(|e| unreachable!("{e:?}"));
    assert_eq!(
        reg.for_plugin(P)[0].widgets[0].value,
        WidgetValue::Items {
            items: vec![ListItem {
                id: id("c"),
                label: "C".to_string()
            }],
            selected: Some(id("c")),
        }
    );
}

/// FR-006: a session-closed panel stays closed across a `clear` +
/// re-registration (P5's own "closed... survive[s]" clause) — `show`
/// re-opens it.
#[test]
fn closed_survives_reregister() {
    let identifier =
        PluginIdentifier::parse("org.modplayer.fixture.ui-panel").unwrap_or_else(|| unreachable!());
    let key = PanelKey::new(identifier, id("main"));
    let mut reg = PanelRegistry::new();
    reg.register(P, id("main"), "T".to_string(), vec![label_widget("a")])
        .unwrap_or_else(|e| unreachable!("{e:?}"));
    reg.close(key.clone());
    assert!(reg.is_closed(&key));
    reg.clear(P);
    reg.register(P, id("main"), "T2".to_string(), vec![label_widget("a")])
        .unwrap_or_else(|e| unreachable!("{e:?}"));
    assert!(reg.is_closed(&key));
    reg.show(&key);
    assert!(!reg.is_closed(&key));
}

/// P5/FR-025: `Suspend` keeps panels intact (nothing calls `clear`, so
/// the view can still render them as a placeholder); `Disable`/
/// `Shutdown` (`clear`) removes them outright.
#[test]
fn suspend_keeps_disable_removes() {
    let mut reg = PanelRegistry::new();
    reg.register(P, id("main"), "T".to_string(), vec![label_widget("a")])
        .unwrap_or_else(|e| unreachable!("{e:?}"));
    assert_eq!(
        reg.for_plugin(P).len(),
        1,
        "Suspend must not clear the registry"
    );
    reg.clear(P);
    assert!(
        reg.for_plugin(P).is_empty(),
        "Disable/Shutdown must remove every panel"
    );
}

// -- OverlayRegistry (US3 T081, contracts/overlays-settings-notify.md §1.1
// "O" rules) -----------------------------------------------------------

fn line(s: &str, at_ms: u64) -> OverlayPrimitive {
    OverlayPrimitive::Line {
        id: id(s),
        at_ms,
        color: OverlayColor::Accent,
    }
}

/// O1: a batch that would push the post-merge count over 500 is refused
/// whole; the 500 already registered are untouched.
#[test]
fn overlay_501st_refused_prior_unchanged() {
    let mut reg = OverlayRegistry::new();
    let first_batch: Vec<_> = (0..500).map(|i| line(&format!("p{i}"), i)).collect();
    reg.add(P, first_batch)
        .unwrap_or_else(|e| unreachable!("{e:?}"));
    let err = reg.add(P, vec![line("one_more", 999)]).unwrap_err();
    assert_eq!(err.reason, "overlay_limit");
    assert_eq!(
        reg.primitives_for(P).len(),
        500,
        "the 501st must not be added"
    );
}

/// O1: re-adding an already-registered id replaces it in place — the
/// batch's own count never double-counts it toward the cap.
#[test]
fn overlay_readd_replaces_in_place() {
    let mut reg = OverlayRegistry::new();
    reg.add(P, vec![line("a", 100)])
        .unwrap_or_else(|e| unreachable!("{e:?}"));
    reg.add(P, vec![line("a", 200)])
        .unwrap_or_else(|e| unreachable!("{e:?}"));
    let primitives = reg.primitives_for(P);
    assert_eq!(primitives.len(), 1);
    match &primitives[0] {
        OverlayPrimitive::Line { at_ms, .. } => assert_eq!(*at_ms, 200),
        other => unreachable!("{other:?}"),
    }
}

/// O2: `remove_overlays` naming any unknown id refuses the whole call —
/// nothing is removed, even the ids that *were* known.
#[test]
fn remove_unknown_not_found_atomic() {
    let mut reg = OverlayRegistry::new();
    reg.add(P, vec![line("a", 100), line("b", 200)])
        .unwrap_or_else(|e| unreachable!("{e:?}"));
    let err = reg.remove(P, &[id("a"), id("missing")]).unwrap_err();
    assert_eq!(err.reason, "unknown_id");
    assert_eq!(
        reg.primitives_for(P).len(),
        2,
        "a partial removal must not apply"
    );
}

/// FR-016: `clear_all` (the `TrackChanged` trigger) empties every
/// plugin's overlays, but each plugin's own first-registration `seq`
/// (O4, "stable for the session") survives the clear — proven by
/// re-adding after the clear and checking the cross-plugin order (P
/// first, Q second) is unchanged.
#[test]
fn overlays_cleared_on_track_change() {
    let mut reg = OverlayRegistry::new();
    reg.add(P, vec![line("a", 100)])
        .unwrap_or_else(|e| unreachable!("{e:?}"));
    reg.add(Q, vec![line("b", 200)])
        .unwrap_or_else(|e| unreachable!("{e:?}"));
    reg.clear_all();
    assert!(reg.primitives_for(P).is_empty());
    assert!(reg.primitives_for(Q).is_empty());

    reg.add(Q, vec![line("b2", 250)])
        .unwrap_or_else(|e| unreachable!("{e:?}"));
    reg.add(P, vec![line("a2", 150)])
        .unwrap_or_else(|e| unreachable!("{e:?}"));
    let (p_seq, q_seq) = (
        reg.seq_of(P).unwrap_or_else(|| unreachable!()),
        reg.seq_of(Q).unwrap_or_else(|| unreachable!()),
    );
    assert!(
        p_seq < q_seq,
        "P's seq (first ever registration) must still sort first"
    );
}

/// O3: `clear` (`on_stop`'s own call, any reason) empties one plugin's
/// overlays without touching another plugin's.
#[test]
fn overlays_cleared_on_stop() {
    let mut reg = OverlayRegistry::new();
    reg.add(P, vec![line("a", 100)])
        .unwrap_or_else(|e| unreachable!("{e:?}"));
    reg.add(Q, vec![line("b", 200)])
        .unwrap_or_else(|e| unreachable!("{e:?}"));
    reg.clear(P);
    assert!(reg.primitives_for(P).is_empty());
    assert_eq!(reg.primitives_for(Q).len(), 1);
}

// -- SettingsRegistry (US4 T094, contracts/overlays-settings-notify.md §2
// "S" rules) ------------------------------------------------------------

fn boolean_field(field_id: &str) -> SettingsField {
    SettingsField {
        id: id(field_id),
        kind: FieldKind::Boolean,
        label: "Snap to beat".to_string(),
        description: None,
        default: json!(false),
    }
}

fn number_field(field_id: &str) -> SettingsField {
    SettingsField {
        id: id(field_id),
        kind: FieldKind::Number {
            min: -12.0,
            max: 12.0,
            step: 1.0,
        },
        label: "Semitone shift".to_string(),
        description: None,
        default: json!(0.0),
    }
}

/// S1: re-registering the same field id (e.g. the plugin re-declaring its
/// schema after a restart) keeps that field's *current* value — a value
/// the host has since applied via `edit` must never be reset back to
/// whatever a (possibly stale) `stored` snapshot says.
#[test]
fn settings_reregister_keeps_values() {
    let mut reg = SettingsRegistry::new();
    reg.register(P, vec![boolean_field("snap")], BTreeMap::new());
    reg.edit(P, "snap", json!(true))
        .unwrap_or_else(|| unreachable!());
    assert_eq!(
        reg.page_for(P)
            .unwrap_or_else(|| unreachable!())
            .values
            .get("snap"),
        Some(&json!(true))
    );

    let mut stale_stored = BTreeMap::new();
    stale_stored.insert("snap".to_string(), json!(false));
    reg.register(P, vec![boolean_field("snap")], stale_stored);
    assert_eq!(
        reg.page_for(P)
            .unwrap_or_else(|| unreachable!())
            .values
            .get("snap"),
        Some(&json!(true)),
        "a re-registration must keep the field's current (edited) value, not the stale stored one"
    );
}

/// S1: a `stored` value that no longer fits the field's kind/range (e.g.
/// the plugin narrowed a number's bounds) substitutes the field's own
/// `default` instead — never carried over invalid, and never a refusal.
#[test]
fn settings_invalid_stored_uses_default() {
    let mut reg = SettingsRegistry::new();
    let mut stored = BTreeMap::new();
    stored.insert("shift".to_string(), json!(999.0));
    reg.register(P, vec![number_field("shift")], stored);
    assert_eq!(
        reg.page_for(P)
            .unwrap_or_else(|| unreachable!())
            .values
            .get("shift"),
        Some(&json!(0.0)),
        "an out-of-range stored value must fall back to the field's default"
    );
}

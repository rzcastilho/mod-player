// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! T109 (US5, 011-plugin-ui-contributions, contracts/overlays-settings-
//! notify.md §3 N3/N2): `notifications::show`'s own attribution
//! rendering — the icon + name before the text, the text's own AccessKit
//! name being the whole resolved Fluent string — and the area's
//! non-modal contract (never a dialog, another widget stays operable
//! alongside it).

use egui::accesskit::{NodeId, Role};
use egui::{Context, RawInput};
use modplayer_core::{NotificationCenter, PluginAttribution, PluginId, Severity};

/// One AccessKit node's accessibility-relevant fields (mirrors
/// `plugin_panels.rs`'s own `AccessNode`): for a `Role::Label` node,
/// `egui`'s own `WidgetInfo` fill sets `value`, not `label`
/// (`accessible_name()` reads whichever is set).
#[derive(Debug, Clone)]
struct AccessNode {
    #[allow(dead_code)]
    id: NodeId,
    role: Role,
    label: Option<String>,
    value: Option<String>,
    is_modal: bool,
}

impl AccessNode {
    fn accessible_name(&self) -> Option<&str> {
        self.label.as_deref().or(self.value.as_deref())
    }
}

fn render_nodes(render: impl FnMut(&mut egui::Ui)) -> Vec<AccessNode> {
    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut render = render;
    let mut output = ctx.run_ui(RawInput::default(), |ui| render(ui));
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    output.drop_without_applying_deltas();

    update
        .nodes
        .iter()
        .map(|(id, node)| AccessNode {
            id: *id,
            role: node.role(),
            label: node.label().map(str::to_string),
            value: node.value().map(str::to_string),
            is_modal: node.is_modal(),
        })
        .collect()
}

fn find_one<'a>(nodes: &'a [AccessNode], role: Role, name: &str) -> &'a AccessNode {
    let matches: Vec<&AccessNode> = nodes
        .iter()
        .filter(|n| n.role == role && n.accessible_name() == Some(name))
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one {role:?} node named `{name}`, found {}: {nodes:?}",
        matches.len()
    );
    matches[0]
}

/// N2/N3: a plugin-attributed notification renders its icon (generic
/// glyph fallback, since nothing is cached in this synthetic frame) and
/// its plugin name before the text — and the text node's own AccessKit
/// name is the whole resolved Fluent string (`"<plugin>: <text>"`), not
/// just the raw text, so a screen reader still gets the full sentence
/// even split across three widgets.
#[test]
fn plugin_notification_attributed() {
    let mut center = NotificationCenter::new();
    center.raise_attributed(
        Severity::Info,
        "plugin-notification",
        vec![
            ("plugin", "Test Plugin".to_string()),
            ("text", "hello from a plugin".to_string()),
        ],
        PluginAttribution {
            id: PluginId(7),
            name: "Test Plugin".to_string(),
        },
    );

    let nodes = render_nodes(|ui| {
        modplayer_ui::notifications::show(ui, &center);
    });

    // The generic-glyph fallback (no cached texture in this synthetic
    // frame) still carries its own non-empty accessible name (FR-014a).
    find_one(
        &nodes,
        Role::Image,
        &modplayer_core::tr("plugin-generic-glyph-desc"),
    );
    // The attribution name, rendered as its own label, before the text.
    find_one(&nodes, Role::Label, "Test Plugin");
    // The text node's own accessible name is the whole resolved Fluent
    // string, not the raw "hello from a plugin" alone.
    let resolved = modplayer_core::tr_args(
        "plugin-notification",
        &[
            ("plugin", "Test Plugin".to_string()),
            ("text", "hello from a plugin".to_string()),
        ],
    );
    find_one(&nodes, Role::Label, &resolved);
}

/// A host-raised (unattributed) notification keeps its own plain
/// rendering: a single label carrying the resolved message, no
/// separate icon/name split.
#[test]
fn host_notification_has_no_attribution_split() {
    let mut center = NotificationCenter::new();
    center.raise(Severity::Info, "no-output-devices");

    let nodes = render_nodes(|ui| {
        modplayer_ui::notifications::show(ui, &center);
    });

    find_one(
        &nodes,
        Role::Label,
        &modplayer_core::tr("no-output-devices"),
    );
}

/// N2: "never a modal" — a plugin notification (of every severity, since
/// `Critical`/`Warning` are the ones most tempted to block) never
/// produces a modal-flagged node, and a sibling widget drawn in the same
/// pass stays fully operable (never disabled, never behind a dialog).
#[test]
fn never_modal() {
    let mut center = NotificationCenter::new();
    for (severity, key) in [
        (Severity::Critical, "critical-msg"),
        (Severity::Warning, "warning-msg"),
        (Severity::Info, "info-msg"),
    ] {
        center.raise_attributed(
            severity,
            "plugin-notification",
            vec![
                ("plugin", "Test Plugin".to_string()),
                ("text", key.to_string()),
            ],
            PluginAttribution {
                id: PluginId(1),
                name: "Test Plugin".to_string(),
            },
        );
    }

    let nodes = render_nodes(|ui| {
        modplayer_ui::notifications::show(ui, &center);
        let _ = ui.button("Elsewhere");
    });

    assert!(
        nodes.iter().all(|n| !n.is_modal),
        "no notification node may be modal-flagged: {nodes:?}"
    );
    let elsewhere = find_one(&nodes, Role::Button, "Elsewhere");
    assert!(
        !elsewhere.is_modal,
        "a sibling widget must stay non-modal alongside the notification area"
    );
}

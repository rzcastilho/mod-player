// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! T109 (US5, 011-plugin-ui-contributions, contracts/overlays-settings-
//! notify.md §3 N3/N2): `notifications::show`'s own attribution
//! rendering — the icon + name before the text, the text's own AccessKit
//! name being the whole resolved Fluent string — and the area's
//! non-modal contract (never a dialog, another widget stays operable
//! alongside it).

use std::collections::HashSet;

use egui::accesskit::{NodeId, Role};
use egui::epaint::ClippedShape;
use egui::{
    Color32, Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Shape, ThemePreference,
};
use modplayer_core::{
    NotificationAction, NotificationCenter, PluginAttribution, PluginId, Severity, tr_args,
};
use modplayer_ui::theme;
use modplayer_ui::theme::tokens::{DARK, DARK_HIGH_CONTRAST, LIGHT, LIGHT_HIGH_CONTRAST};

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
    bounds: Option<Rect>,
}

impl AccessNode {
    fn accessible_name(&self) -> Option<&str> {
        self.label.as_deref().or(self.value.as_deref())
    }
}

fn render_nodes(render: impl FnMut(&mut egui::Ui)) -> Vec<AccessNode> {
    let ctx = Context::default();
    ctx.enable_accesskit();
    render_nodes_on(&ctx, RawInput::default(), render)
}

/// Same as [`render_nodes`], but on a caller-supplied `Context`/`RawInput`
/// (T022/US4): reused across frames so a click's press and release land on
/// the same widget ids and memory — a fresh `Context` each frame, as
/// `render_nodes` uses, cannot register a click at all.
fn render_nodes_on(
    ctx: &Context,
    input: RawInput,
    mut render: impl FnMut(&mut egui::Ui),
) -> Vec<AccessNode> {
    let mut output = ctx.run_ui(input, |ui| render(ui));
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
            bounds: node.bounds().map(|b| {
                Rect::from_min_max(
                    Pos2::new(b.x0 as f32, b.y0 as f32),
                    Pos2::new(b.x1 as f32, b.y1 as f32),
                )
            }),
        })
        .collect()
}

/// A window-sized `RawInput` (T022): card width caps at 360 px once the
/// window is at least `360 + 16` wide (contract S4), so 800 px gives a
/// stable, unambiguous card width for the truncation tests below.
fn sized_input(w: f32, h: f32) -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(w, h))),
        ..Default::default()
    }
}

/// Press then release the primary button at `pos`, on the same `ctx`
/// (mirrors `tests/notification_stack.rs`'s own `click`), re-running
/// `render` (which re-draws `notifications::show` into `state`) across
/// both frames.
fn click(ctx: &Context, w: f32, h: f32, pos: Pos2, mut render: impl FnMut(&mut egui::Ui)) {
    let mut press = sized_input(w, h);
    press.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::default(),
    });
    let output = ctx.run_ui(press, |ui| render(ui));
    output.drop_without_applying_deltas();

    let mut release = sized_input(w, h);
    release.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::default(),
    });
    let output = ctx.run_ui(release, |ui| render(ui));
    output.drop_without_applying_deltas();
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

    let mut stack_state = modplayer_ui::notifications::StackState::default();
    let nodes = render_nodes(|ui| {
        modplayer_ui::notifications::show(ui, &center, &mut stack_state);
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

    let mut stack_state = modplayer_ui::notifications::StackState::default();
    let nodes = render_nodes(|ui| {
        modplayer_ui::notifications::show(ui, &center, &mut stack_state);
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

    let mut stack_state = modplayer_ui::notifications::StackState::default();
    let nodes = render_nodes(|ui| {
        modplayer_ui::notifications::show(ui, &center, &mut stack_state);
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

// ---------------------------------------------------------------------
// SC-003 (US3, T013, contract S4, research R7, FR-007/FR-008): severity is
// legible via accent-bar/icon colour + distinct glyph in every theme, on
// top of the severity word already covered by `accessibility.rs`.
// ---------------------------------------------------------------------

/// One notification's rendered shapes, with `roles` (light/dark/high
/// contrast, per `theme::apply_tokens_for` + `Context::set_theme`) applied
/// exactly as `App::update` applies it every frame.
fn render_severity_shapes(
    severity: Severity,
    dark: bool,
    high_contrast: bool,
) -> Vec<ClippedShape> {
    let mut center = NotificationCenter::new();
    center.raise(severity, "no-output-devices");

    let ctx = Context::default();
    theme::apply_tokens_for(&ctx, high_contrast);
    ctx.set_theme(if dark {
        ThemePreference::Dark
    } else {
        ThemePreference::Light
    });

    let mut stack_state = modplayer_ui::notifications::StackState::default();
    let mut output = ctx.run_ui(RawInput::default(), |ui| {
        let _ = modplayer_ui::notifications::show(ui, &center, &mut stack_state);
    });
    let shapes = std::mem::take(&mut output.shapes);
    output.drop_without_applying_deltas();
    shapes
}

/// Every `Shape::Rect` narrow enough to be the 4 px accent bar (contract
/// S4), regardless of its rounded-corner clamping.
fn accent_bar_fills(shapes: &[ClippedShape]) -> Vec<Color32> {
    shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            Shape::Rect(r) if r.rect.width() <= 4.5 => Some(r.fill),
            _ => None,
        })
        .collect()
}

/// Every colour any text run in `shapes` resolved to (mirrors `rows.rs`'s
/// own `text_colors` helper): `RichText::color`'s run bakes straight into
/// the `Galley`'s `LayoutJob` sections at layout time.
fn text_colors(shapes: &[ClippedShape]) -> HashSet<Color32> {
    shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            Shape::Text(t) => Some(t.galley.job.sections.iter().map(|s| s.format.color)),
            _ => None,
        })
        .flatten()
        .collect()
}

/// SC-003: Critical's accent bar + icon colour is `Roles::danger`,
/// Info's is `Roles::positive`, Warning's is `Roles::warning` — and they
/// differ from one another — in light, dark and both high-contrast tables
/// (017-high-contrast-appearance).
#[test]
fn severity_accent_and_icon_colours_match_the_role_in_every_theme() {
    for (dark, high_contrast, roles) in [
        (false, false, &LIGHT),
        (true, false, &DARK),
        (false, true, &LIGHT_HIGH_CONTRAST),
        (true, true, &DARK_HIGH_CONTRAST),
    ] {
        assert_ne!(
            roles.danger, roles.positive,
            "dark={dark} hc={high_contrast}"
        );
        assert_ne!(
            roles.danger, roles.warning,
            "dark={dark} hc={high_contrast}"
        );
        assert_ne!(
            roles.warning, roles.positive,
            "dark={dark} hc={high_contrast}"
        );

        for (severity, expected, label) in [
            (Severity::Critical, roles.danger, "danger"),
            (Severity::Warning, roles.warning, "warning"),
            (Severity::Info, roles.positive, "positive"),
        ] {
            let shapes = render_severity_shapes(severity, dark, high_contrast);

            let bars = accent_bar_fills(&shapes);
            assert!(
                bars.contains(&expected),
                "dark={dark} hc={high_contrast} {severity:?}: expected a {label}-coloured \
                 accent bar, found {bars:?}"
            );

            let texts = text_colors(&shapes);
            assert!(
                texts.contains(&expected),
                "dark={dark} hc={high_contrast} {severity:?}: expected the icon glyph \
                 coloured {label}, found {texts:?}"
            );
        }
    }
}

/// SC-003: the icon glyph itself differs between severities (`⛔` vs `⚠`
/// vs `ℹ`) — colour is never the only distinguishing cue.
#[test]
fn severity_icon_glyphs_differ_between_severities() {
    for (severity, glyph) in [
        (Severity::Critical, "⛔"),
        (Severity::Warning, "⚠"),
        (Severity::Info, "ℹ"),
    ] {
        let mut center = NotificationCenter::new();
        center.raise(severity, "no-output-devices");
        let mut stack_state = modplayer_ui::notifications::StackState::default();
        let nodes = render_nodes(|ui| {
            modplayer_ui::notifications::show(ui, &center, &mut stack_state);
        });
        find_one(&nodes, Role::Label, glyph);
    }
}

// ---------------------------------------------------------------------
// T022 (US4, SC-004/SC-006, contract S5/S6/S8, FR-010/FR-013/FR-015): the
// message truncates to two lines with a "Show more" toggle, "Details"
// stays hidden (and the raw device id with it) until opened, and every
// existing action still dispatches from the new card layout.
// ---------------------------------------------------------------------

/// `true` iff any of `id`'s own identifying tokens (its alphanumeric runs,
/// split on `:`/`{`/`}`/`-`, at least `min_len` chars long) shows up whole,
/// case-sensitively, in `haystack` — the substring-leak check contract
/// fluent-strings.md's SC-004 asks for ("no substring of length ≥ 4 of the
/// id"). Whole-token rather than a raw sliding window: the UX-38 example
/// id's own `...EngineOutputDP...` token coincidentally shares 4 lowercase
/// letters with the *fixed, production* `notification-device-fallback-
/// default` wording ("the system default **output**") — an English-word
/// coincidence, not an id leak, which a same-case, whole-token check
/// correctly lets through.
fn contains_id_fragment(haystack: &str, id: &str, min_len: usize) -> bool {
    id.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| token.chars().count() >= min_len)
        .any(|token| haystack.contains(token))
}

/// SC-004/S5/S6: a device notification with a long (40-char) device name
/// and a raw-id `detail` — collapsed, the message wraps to two lines with
/// "Show more" and "Details" present but the raw id nowhere in the
/// accessible tree; "Show more"/"Details" reveal the full text/raw id and
/// relabel to "Show less"/"Hide details"; the message label's accessible
/// name is the full resolved text throughout, never the elided display
/// text (contract S8).
#[test]
fn message_truncates_with_show_more_and_details_stay_hidden_until_opened() {
    // The UX-38 example id from contract fluent-strings.md's own SC-004 note.
    let raw_id = "coreaudio:AppleGFXHDAEngineOutputDP:10001:0:{6D1E-7715-00097FED}";
    let device_name: String = "A".repeat(40);
    let args = vec![
        ("device", device_name),
        ("fallback", "the system default output".to_string()),
    ];

    let mut center = NotificationCenter::new();
    center.raise_with_detail(
        Severity::Warning,
        "device-missing-at-launch",
        args.clone(),
        raw_id.to_string(),
    );
    let resolved = tr_args("device-missing-at-launch", &args);

    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut state = modplayer_ui::notifications::StackState::default();
    let (w, h) = (800.0_f32, 600.0_f32);

    // Collapsed: the message's accessible name is the full resolved text
    // (never the visibly elided one), "Show more" and "Details" both
    // present, and the raw id is nowhere in the tree yet.
    let nodes = render_nodes_on(&ctx, sized_input(w, h), |ui| {
        let _ = modplayer_ui::notifications::show(ui, &center, &mut state);
    });
    find_one(&nodes, Role::Label, &resolved);
    let show_more = find_one(
        &nodes,
        Role::Button,
        &modplayer_core::tr("notification-show-more"),
    );
    assert!(
        show_more.accessible_name().is_some_and(|n| !n.is_empty()),
        "\"Show more\" must have a non-empty accessible name"
    );
    find_one(
        &nodes,
        Role::Button,
        &modplayer_core::tr("notification-details"),
    );
    assert!(
        nodes.iter().all(|n| !n
            .accessible_name()
            .is_some_and(|name| contains_id_fragment(name, raw_id, 4))),
        "the raw device id must not leak into the collapsed accessible tree: {nodes:?}"
    );

    // Toggle "Show more" -> "Show less"; the message's accessible name is
    // unchanged (it was already the full text).
    let show_more_pos = show_more.bounds.expect("bounds").center();
    click(&ctx, w, h, show_more_pos, |ui| {
        let _ = modplayer_ui::notifications::show(ui, &center, &mut state);
    });
    let nodes = render_nodes_on(&ctx, sized_input(w, h), |ui| {
        let _ = modplayer_ui::notifications::show(ui, &center, &mut state);
    });
    assert!(
        find_one(
            &nodes,
            Role::Button,
            &modplayer_core::tr("notification-show-less")
        )
        .accessible_name()
        .is_some_and(|n| !n.is_empty()),
        "\"Show less\" must have a non-empty accessible name"
    );
    find_one(&nodes, Role::Label, &resolved);

    // Toggle "Details" -> "Hide details"; the raw id now appears.
    let details_pos = find_one(
        &nodes,
        Role::Button,
        &modplayer_core::tr("notification-details"),
    )
    .bounds
    .expect("Details button must have bounds")
    .center();
    click(&ctx, w, h, details_pos, |ui| {
        let _ = modplayer_ui::notifications::show(ui, &center, &mut state);
    });
    let nodes = render_nodes_on(&ctx, sized_input(w, h), |ui| {
        let _ = modplayer_ui::notifications::show(ui, &center, &mut state);
    });
    assert!(
        find_one(
            &nodes,
            Role::Button,
            &modplayer_core::tr("notification-hide-details")
        )
        .accessible_name()
        .is_some_and(|n| !n.is_empty()),
        "\"Hide details\" must have a non-empty accessible name"
    );
    assert!(
        nodes
            .iter()
            .any(|n| n.value.as_deref() == Some(raw_id) || n.label.as_deref() == Some(raw_id)),
        "the raw id must be visible once Details is open: {nodes:?}"
    );
}

/// S5/S6 negative case: a short, plain notification (no long device name,
/// no `detail`) never renders "Show more"/"Show less" or "Details"/"Hide
/// details" — the toggles are present iff their condition holds.
#[test]
fn short_plain_message_has_no_truncation_or_details_controls() {
    let mut center = NotificationCenter::new();
    center.raise(Severity::Critical, "sample-notification-critical");

    let mut state = modplayer_ui::notifications::StackState::default();
    let nodes = render_nodes(|ui| {
        let _ = modplayer_ui::notifications::show(ui, &center, &mut state);
    });

    for key in [
        "notification-show-more",
        "notification-show-less",
        "notification-details",
        "notification-hide-details",
    ] {
        assert!(
            !nodes.iter().any(|n| n.role == Role::Button
                && n.accessible_name() == Some(modplayer_core::tr(key).as_str())),
            "unexpected \"{key}\" control on a short, detail-less card: {nodes:?}"
        );
    }
}

/// SC-006: every existing notification action — Sign in, Open status
/// page, Retry, Open upgrade page, Restart plugin, Disable plugin — still
/// dispatches its `NotificationAction` from the new card layout.
#[test]
fn every_existing_action_still_dispatches_from_the_new_layout() {
    let plugin_id = PluginId(3);
    let cases: [(Vec<NotificationAction>, [&str; 2]); 3] = [
        (
            vec![
                NotificationAction::SignIn,
                NotificationAction::OpenStatusPage,
            ],
            ["notification-action-sign-in", "action-status-page"],
        ),
        (
            vec![
                NotificationAction::RetrySource,
                NotificationAction::OpenUpgradePage,
            ],
            ["action-retry", "action-open-upgrade-page"],
        ),
        (
            vec![
                NotificationAction::RestartPlugin(plugin_id),
                NotificationAction::DisablePlugin(plugin_id),
            ],
            [
                "notification-action-restart-plugin",
                "notification-action-disable-plugin",
            ],
        ),
    ];

    for (actions, keys) in cases {
        let mut center = NotificationCenter::new();
        let id = center.raise_with_actions(
            Severity::Warning,
            "sample-notification-warning",
            Vec::new(),
            actions.clone(),
        );

        let ctx = Context::default();
        ctx.enable_accesskit();
        let mut state = modplayer_ui::notifications::StackState::default();
        let (w, h) = (800.0_f32, 600.0_f32);

        for (action, key) in actions.iter().zip(keys.iter()) {
            let nodes = render_nodes_on(&ctx, sized_input(w, h), |ui| {
                let _ = modplayer_ui::notifications::show(ui, &center, &mut state);
            });
            let pos = find_one(&nodes, Role::Button, &modplayer_core::tr(key))
                .bounds
                .unwrap_or_else(|| panic!("`{key}` button must have bounds"))
                .center();

            let mut interaction = modplayer_ui::notifications::NotificationInteraction::default();
            let mut press = sized_input(w, h);
            press.events.push(Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::default(),
            });
            let output = ctx.run_ui(press, |ui| {
                let _ = modplayer_ui::notifications::show(ui, &center, &mut state);
            });
            output.drop_without_applying_deltas();
            let mut release = sized_input(w, h);
            release.events.push(Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::default(),
            });
            let output = ctx.run_ui(release, |ui| {
                interaction = modplayer_ui::notifications::show(ui, &center, &mut state);
            });
            output.drop_without_applying_deltas();

            assert_eq!(
                interaction.action_clicked,
                Some((id, *action)),
                "clicking `{key}` must report {action:?} for notification {id}"
            );
        }
    }
}

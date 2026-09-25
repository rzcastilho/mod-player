// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! `shell::Chrome`/`shell::show_chrome` integration tests (020-shell-
//! navigation-and-gates, US1, contracts/shell-chrome.md C1-C9): the rail
//! is never added to the frame during a launch gate or the Settings "Test
//! output device" preview (contract C2), the dispatcher gate the same
//! predicate drives never defers a nav shortcut across the gate/Main
//! boundary (contract C3), the step indicator is one correctly-valued
//! `ProgressIndicator` AccessKit node whose position depends only on
//! `LaunchStep` (contract C4), and the Settings-triggered Device Check
//! preview hides the rail exactly like a launch gate (contract C9).
//!
//! `App` cannot be constructed headlessly (research.md R1: it needs a real
//! `eframe::CreationContext`), so every test here drives
//! `shell::Chrome::for_frame`/`shell::show_chrome` directly against a bare
//! `egui::Context`, the same pattern `shell.rs`'s own unit tests and
//! `tests/actions.rs` already use for the dispatcher.

use egui::{CentralPanel, Color32, Context, Id, Pos2, RawInput, Rect, vec2};
use modplayer_account::LaunchStep;
use modplayer_core::tr;
use modplayer_ui::actions::{self, FocusClaims};
use modplayer_ui::shell::{self, Chrome, GateStep, Section, Shell};
use modplayer_ui::theme::controls as theme_controls;
use modplayer_ui::theme::tokens::LIGHT;

const SCREEN_WIDTH: f32 = 1200.0;
const SCREEN_HEIGHT: f32 = 800.0;

fn screen_input() -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(
            Pos2::ZERO,
            vec2(SCREEN_WIDTH, SCREEN_HEIGHT),
        )),
        ..Default::default()
    }
}

/// The five nav rail labels (`shell.rs`'s `SECTIONS`), resolved once so a
/// test can assert none of them appear anywhere in a gate frame's
/// AccessKit tree.
fn nav_labels() -> Vec<String> {
    [
        "nav-library",
        "nav-search",
        "nav-now-playing",
        "nav-plugins",
        "nav-settings",
    ]
    .into_iter()
    .map(tr)
    .collect()
}

/// Render one frame exactly like `App::ui`'s normative order (contracts/
/// shell-chrome.md): `show_chrome` first, then a `CentralPanel` standing in
/// for the gate/Main content. Returns the `CentralPanel`'s content rect
/// (contract C2's "central content rect") and, when AccessKit is enabled
/// on `ctx`, the frame's `accesskit::TreeUpdate`.
fn render_chrome_frame(
    ctx: &Context,
    chrome: Chrome,
    shell: &mut Shell,
) -> (Rect, Option<egui::accesskit::TreeUpdate>) {
    let mut central_rect = Rect::NOTHING;
    let mut output = ctx.run_ui(screen_input(), |ui| {
        shell::show_chrome(ui, chrome, shell);
        let response = CentralPanel::default().show(ui, |ui| {
            ui.label("gate content");
        });
        central_rect = response.response.rect;
    });
    let update = output.platform_output.accesskit_update.take();
    output.drop_without_applying_deltas();
    (central_rect, update)
}

fn fresh_accesskit_context() -> Context {
    let ctx = Context::default();
    modplayer_ui::theme::apply_tokens(&ctx);
    ctx.enable_accesskit();
    ctx
}

// -- Contract C2: rail absent, full-width central content, no nav-* node --

/// Every `LaunchStep` that is not `Main` hides the rail (Welcome, SignIn,
/// DeviceCheck): no `shell-nav-rail` `Panel` in egui memory, no AccessKit
/// node labelled a nav item, and the central content spans the full
/// screen width (FR-001, FR-004, SC-001).
#[test]
fn rail_is_absent_during_every_launch_gate_step() {
    for step in [
        LaunchStep::Welcome,
        LaunchStep::SignIn,
        LaunchStep::DeviceCheck,
    ] {
        let ctx = fresh_accesskit_context();
        let mut shell = Shell::default();
        let chrome = Chrome::for_frame(step, false);
        assert!(!chrome.rail, "step={step:?} must hide the rail");

        let (central_rect, update) = render_chrome_frame(&ctx, chrome, &mut shell);

        assert!(
            egui::containers::panel::PanelState::load(&ctx, Id::new("shell-nav-rail")).is_none(),
            "step={step:?}: no shell-nav-rail Panel state should exist in egui memory"
        );
        assert!(
            (central_rect.left() - 0.0).abs() < 0.5,
            "step={step:?}: central content must start at the window's left margin, got {}",
            central_rect.left()
        );

        let update = update.expect("accesskit_update should be populated once enabled");
        let labels = nav_labels();
        for (_, node) in &update.nodes {
            if let Some(label) = node.label() {
                assert!(
                    !labels.iter().any(|nav| nav == label),
                    "step={step:?}: found a nav-rail-labelled node ({label:?}) while the rail is hidden"
                );
            }
        }
    }
}

/// Contract C9: the Settings-triggered "Test output device" preview
/// (`Main` step, `device_check_open = true`) yields the exact same
/// `Chrome{rail:false, gate:None}` a launch gate does — no rail, no step
/// indicator — and closing it (`device_check_open = false` the next
/// frame) restores the rail.
#[test]
fn device_check_preview_hides_and_then_restores_the_rail() {
    let ctx = fresh_accesskit_context();
    let mut shell = Shell::default();

    let preview_chrome = Chrome::for_frame(LaunchStep::Main, true);
    assert_eq!(
        preview_chrome,
        Chrome {
            rail: false,
            gate: None
        },
        "a Settings-triggered Device Check preview must hide both the rail and the step indicator"
    );
    let (central_rect, _) = render_chrome_frame(&ctx, preview_chrome, &mut shell);
    assert!(
        (central_rect.left() - 0.0).abs() < 0.5,
        "the preview frame's central content must span the full window"
    );
    assert!(
        egui::containers::panel::PanelState::load(&ctx, Id::new("shell-nav-rail")).is_none(),
        "no shell-nav-rail Panel state should exist while the preview is open"
    );

    // Closing the preview (device_check now None) restores the rail the
    // very next frame.
    let restored_chrome = Chrome::for_frame(LaunchStep::Main, false);
    assert_eq!(
        restored_chrome,
        Chrome {
            rail: true,
            gate: None
        }
    );
    let (central_rect, _) = render_chrome_frame(&ctx, restored_chrome, &mut shell);
    assert!(
        central_rect.left() > 0.0,
        "once the rail is back, the central content must start after it, not at the window edge"
    );
    assert!(
        egui::containers::panel::PanelState::load(&ctx, Id::new("shell-nav-rail")).is_some(),
        "the shell-nav-rail Panel state must exist again once the rail is restored"
    );
}

// -- Contracts C5/C6: rail item set and selected-item paint --------------

/// Contract C5 (also proves library-tab-counts L4, "no count badge on the
/// rail"): with the rail shown, exactly five `Role::Button` nodes exist,
/// their labels the exact `SECTIONS` set, appearing top-to-bottom in
/// `SECTIONS` order — and no digit-only label (a count badge) anywhere.
#[test]
fn rail_shows_exactly_the_five_sections_in_order_no_badges() {
    let ctx = fresh_accesskit_context();
    let mut shell = Shell::default();
    let chrome = Chrome::for_frame(LaunchStep::Main, false);

    let (_, update) = render_chrome_frame(&ctx, chrome, &mut shell);
    let update = update.expect("accesskit_update should be populated once enabled");

    let labels = nav_labels();
    let mut buttons: Vec<(String, f32)> = update
        .nodes
        .iter()
        .filter(|(_, node)| node.role() == egui::accesskit::Role::Button)
        .filter_map(|(_, node)| {
            let label = node.label()?.to_string();
            let top = node.bounds()?.y0 as f32;
            Some((label, top))
        })
        .collect();
    assert_eq!(
        buttons.len(),
        labels.len(),
        "expected exactly {} Button nodes on the rail, found {}: {buttons:?}",
        labels.len(),
        buttons.len()
    );
    buttons.sort_by(|a, b| a.1.total_cmp(&b.1));
    let ordered_labels: Vec<String> = buttons.into_iter().map(|(l, _)| l).collect();
    assert_eq!(
        ordered_labels, labels,
        "the rail's five items must appear top-to-bottom in SECTIONS order"
    );

    // No count-badge node anywhere on the rail (Clarification 14,
    // library-tab-counts L4): every label is one of the five section
    // names, never a bare digit string.
    for (_, node) in &update.nodes {
        if let Some(label) = node.label()
            && !label.is_empty()
        {
            assert!(
                !label.chars().all(|c| c.is_ascii_digit()),
                "found a digit-only label on the rail (looks like a count badge): {label:?}"
            );
        }
    }
}

/// Contract C6: the selected rail item paints exactly one leading-edge
/// `nav_indicator` line (spanning its full height, at its left edge), no
/// filled background, and a `text_primary` label; every unselected item
/// paints neither a fill nor an indicator, and a `text_secondary` label.
#[test]
fn selected_rail_item_shows_indicator_not_fill_others_show_neither() {
    let ctx = Context::default();
    modplayer_ui::theme::apply_tokens(&ctx);
    ctx.set_theme(egui::ThemePreference::from(egui::Theme::Light));
    ctx.enable_accesskit();
    let roles = LIGHT;

    let mut shell = Shell::default();
    assert_eq!(shell.section, Section::Library, "test setup");
    let chrome = Chrome::for_frame(LaunchStep::Main, false);

    let mut output = ctx.run_ui(screen_input(), |ui| {
        shell::show_chrome(ui, chrome, &mut shell);
    });
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");

    // Each nav item's bounds, by its exact (un-uppercased) `tr(key)` label,
    // in `SECTIONS` order (index 0 == Library == selected).
    let labels = nav_labels();
    let bounds: Vec<Rect> = labels
        .iter()
        .map(|label| {
            update
                .nodes
                .iter()
                .find(|(_, node)| node.label() == Some(label.as_str()))
                .and_then(|(_, node)| node.bounds())
                .map(|b| {
                    Rect::from_min_max(
                        Pos2::new(b.x0 as f32, b.y0 as f32),
                        Pos2::new(b.x1 as f32, b.y1 as f32),
                    )
                })
                .unwrap_or_else(|| panic!("expected a Button node labelled {label:?}"))
        })
        .collect();

    let indicator = theme_controls::nav_indicator(&roles);
    let indicator_lines: Vec<Rect> = output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::LineSegment { points, stroke } if stroke.color == indicator.color => {
                Some(Rect::from_two_pos(points[0], points[1]))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        indicator_lines.len(),
        1,
        "exactly one nav-indicator-coloured line must be painted (the selected item's), got {indicator_lines:?}"
    );
    let indicator_line = indicator_lines[0];
    assert!(
        (indicator_line.left() - bounds[0].left()).abs() < 0.5,
        "the indicator line must sit at the selected (Library) item's left edge: {indicator_line:?} vs {:?}",
        bounds[0]
    );
    assert!(
        (indicator_line.top() - bounds[0].top()).abs() < 0.5
            && (indicator_line.bottom() - bounds[0].bottom()).abs() < 0.5,
        "the indicator line must span the selected item's full height: {indicator_line:?} vs {:?}",
        bounds[0]
    );

    // No filled rect anywhere within any nav item's bounds — no pointer
    // moved this frame, so no hover fill either, selected or not.
    for (i, rect) in bounds.iter().enumerate() {
        let has_fill = output.shapes.iter().any(|clipped| match &clipped.shape {
            egui::Shape::Rect(r) => r.fill != Color32::TRANSPARENT && rect.contains_rect(r.rect),
            _ => false,
        });
        assert!(
            !has_fill,
            "nav item {i} ({:?}) must paint no filled background",
            labels[i]
        );
    }

    // Label colours: the selected (Library, index 0) item is text_primary;
    // every other item is text_secondary.
    let text_color_at = |rect: &Rect| -> Color32 {
        output
            .shapes
            .iter()
            .find_map(|clipped| match &clipped.shape {
                egui::Shape::Text(t) if rect.contains(t.pos) => {
                    t.galley.job.sections.first().map(|s| s.format.color)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("expected a text run inside {rect:?}"))
    };
    assert_eq!(
        text_color_at(&bounds[0]),
        roles.text_primary,
        "the selected item's label must be text_primary"
    );
    for (i, rect) in bounds.iter().enumerate().skip(1) {
        assert_eq!(
            text_color_at(rect),
            roles.text_secondary,
            "unselected item {i} ({:?}) label must be text_secondary",
            labels[i]
        );
    }

    output.drop_without_applying_deltas();
}

// -- Contract C3: the dispatcher gate never defers a nav shortcut --------

/// Pressing `Cmd/Ctrl+1..5` while `Chrome::navigation_enabled()` is false
/// (any gate step) must produce no invocation *and* leave `shell.section`
/// unchanged on a later `Main` frame with no input — the predicate that
/// hides the rail is the same one that gates `actions::dispatch`
/// (`App::ui`'s `pre.navigation_enabled()`), so nothing is deferred across
/// the gate/Main boundary (FR-001b).
#[test]
fn dispatcher_gate_never_defers_a_shortcut_across_the_gate_boundary() {
    let ctx = Context::default();
    let shell = Shell::default();
    let claims = FocusClaims::default();
    let scope = modplayer_core::actions::ScopeState {
        now_playing_shown: false,
        marker_focused: false,
    };

    // Frame 1: a gate step (Welcome). `App::ui` would compute
    // `pre = Chrome::for_frame(step, ..)` and only dispatch when
    // `pre.navigation_enabled()` — here that's false, so `dispatch` is
    // never even called; the Cmd+1..5 events are simply not consumed by
    // the action system this frame.
    let gate_chrome = Chrome::for_frame(LaunchStep::Welcome, false);
    assert!(!gate_chrome.navigation_enabled());

    let mut input = screen_input();
    input.events = vec![
        egui::Event::Key {
            key: egui::Key::Num1,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        },
        egui::Event::Key {
            key: egui::Key::Num5,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        },
    ];
    let mut invoked_during_gate = false;
    let output = ctx.run_ui(input, |ui| {
        let frame_ctx = ui.ctx().clone();
        if gate_chrome.navigation_enabled() {
            let registry = fresh_registry();
            let invocations = actions::dispatch(&frame_ctx, &claims, &registry, &scope);
            invoked_during_gate = !invocations.is_empty();
        }
    });
    output.drop_without_applying_deltas();
    assert!(
        !invoked_during_gate,
        "dispatch must not run at all during a gate frame"
    );
    assert_eq!(
        shell.section,
        Section::Library,
        "shell.section must be untouched after the gate frame"
    );

    // Frame 2: now `Main`, no input at all — proves the two Cmd+N presses
    // above were not queued anywhere and replayed once navigation opened
    // up (no deferred invocation).
    let main_chrome = Chrome::for_frame(LaunchStep::Main, false);
    assert!(main_chrome.navigation_enabled());
    let claims2 = FocusClaims::default();
    let output = ctx.run_ui(screen_input(), |ui| {
        let frame_ctx = ui.ctx().clone();
        let registry = fresh_registry();
        let invocations = actions::dispatch(&frame_ctx, &claims2, &registry, &scope);
        assert!(
            invocations.is_empty(),
            "a later Main frame with no input must produce no invocation — nothing was deferred"
        );
    });
    output.drop_without_applying_deltas();
    assert_eq!(shell.section, Section::Library, "still untouched");
}

/// A minimal `ActionRegistry` (the same shape `PlaybackController::
/// actions()` would hand `dispatch`) — built without a controller, since
/// this test only needs `dispatch`'s own gating behaviour, not any actual
/// host action effect.
fn fresh_registry() -> modplayer_core::actions::ActionRegistry {
    modplayer_core::actions::ActionRegistry::new(modplayer_core::actions::KeymapOverrides::default())
}

// -- Contract C4: the step indicator's AccessKit ProgressIndicator node --

/// During every launch gate step, exactly one `Role::ProgressIndicator`
/// AccessKit node exists, named `"Step {n} of 3: {label}"` with
/// `numeric_value = n`/`max_numeric_value = 3`, and no `Role::Button` node
/// exists (the indicator contributes no focusable node, and the rail is
/// absent). Re-rendering the same step twice (standing in for a retry
/// sub-state, which never changes `LaunchStep`) leaves `n` unchanged.
#[test]
fn step_indicator_is_one_correctly_valued_progress_node() {
    let cases = [
        (
            LaunchStep::Welcome,
            GateStep::Welcome,
            1u8,
            "gate-step-welcome",
        ),
        (
            LaunchStep::SignIn,
            GateStep::SignIn,
            2u8,
            "gate-step-sign-in",
        ),
        (
            LaunchStep::DeviceCheck,
            GateStep::AudioOutputCheck,
            3u8,
            "gate-step-audio-output-check",
        ),
    ];
    for (step, gate_step, n, label_key) in cases {
        let ctx = fresh_accesskit_context();
        let mut shell = Shell::default();
        let chrome = Chrome::for_frame(step, false);
        assert_eq!(chrome.gate, Some(gate_step));

        let expected_label = modplayer_core::tr_args(
            "gate-step-progress",
            &[
                ("current", n.to_string()),
                ("total", shell::GATE_STEP_TOTAL.to_string()),
                ("label", tr(label_key)),
            ],
        );

        // Render twice — a stand-in for a retry/sub-view change, which
        // never touches `LaunchStep` and so must never touch `n`.
        for _ in 0..2 {
            let (_, update) = render_chrome_frame(&ctx, chrome, &mut shell);
            let update = update.expect("accesskit_update should be populated once enabled");

            let mut progress_nodes = 0;
            let mut button_nodes = 0;
            for (_, node) in &update.nodes {
                match node.role() {
                    egui::accesskit::Role::ProgressIndicator => {
                        progress_nodes += 1;
                        assert_eq!(node.label(), Some(expected_label.as_str()));
                        assert_eq!(node.numeric_value(), Some(f64::from(n)));
                        assert_eq!(
                            node.max_numeric_value(),
                            Some(f64::from(shell::GATE_STEP_TOTAL))
                        );
                    }
                    egui::accesskit::Role::Button => button_nodes += 1,
                    _ => {}
                }
            }
            assert_eq!(
                progress_nodes, 1,
                "step={step:?}: exactly one ProgressIndicator node expected"
            );
            assert_eq!(
                button_nodes, 0,
                "step={step:?}: no Button-role node while the rail is hidden"
            );
        }
    }
}

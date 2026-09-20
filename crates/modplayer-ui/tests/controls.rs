// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! Settings › Controls screen tests (007, US2, T062-T066; contracts/
//! ui-actions.md §4/§5/§7, data-model.md §4.3): the filter, the whole
//! catalog grouped by category, capture-mode acceptance/rejection, chip
//! removal, per-action/page-level reset, disabled-row rendering, and
//! platform-glyph display. US3's `⚠`/conflict-partner surfacing (T073,
//! T075) lives at the bottom of this file.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use egui::accesskit::{Role, TreeUpdate};
use egui::{Context, Event, Key as EguiKey, Modifiers, PointerButton, Pos2, RawInput, Rect};
use modplayer_audio_io::{FakeBackend, OutputBackend};
use modplayer_audio_source::SourceHost;
use modplayer_audio_source_synthetic::SyntheticHost;
use modplayer_capability_gateway::ui::WidgetValue;
use modplayer_core::actions::{
    ActionCategory, ActionId, Chord, HostAction, KeyName, Platform, PluginActionId, ScopeState,
};
use modplayer_core::plugins::{Lifecycle, PluginId};
use modplayer_core::settings::SettingsStore;
use modplayer_core::{PlaybackController, tr, tr_args};
use modplayer_ui::settings::controls::{self, CaptureReject, CaptureRule, ControlsScreen};

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-ui-controls-{label}-{}-{unique}",
            std::process::id(),
        ));
        let _ = std::fs::create_dir_all(&dir);
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fresh_store(label: &str) -> (SettingsStore, TempDir) {
    let dir = TempDir::new(label);
    let store = SettingsStore::with_path(dir.path().join("settings.toml"));
    (store, dir)
}

/// A bare controller with no device/track state: the Controls screen only
/// ever reads/mutates `controller.actions()`, so nothing else needs
/// setting up (mirrors `settings/playback.rs`'s own minimal fixture).
fn fresh_controller(label: &str) -> (PlaybackController<FakeBackend, SyntheticHost>, TempDir) {
    let (store, dir) = fresh_store(label);
    let controller =
        PlaybackController::new(FakeBackend::new(vec![]), SyntheticHost::new(44_100), store);
    (controller, dir)
}

fn default_input() -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(900.0, 4000.0))),
        ..Default::default()
    }
}

/// This process's platform, exactly as `settings/controls.rs::show`
/// computes it — used to build the expected display string for a chord
/// (research R14; `Chord::display`'s own dual-branch coverage lives in
/// `modplayer-core`'s `chord_display_mac_and_other`).
fn platform_now() -> Platform {
    let ctx = Context::default();
    if ctx.os().is_mac() {
        Platform::Mac
    } else {
        Platform::Other
    }
}

fn key(key: EguiKey, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

/// Render `controls::show` once and return the raw AccessKit update
/// (mirrors `queue_view.rs`'s own helper).
fn render_update<B: OutputBackend, H: SourceHost>(
    ctx: &Context,
    controller: &mut PlaybackController<B, H>,
    screen: &mut ControlsScreen,
) -> TreeUpdate {
    let mut output = ctx.run_ui(default_input(), |ui| {
        controls::show(ui, controller, screen, None);
    });
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    output.drop_without_applying_deltas();
    update
}

/// Every non-empty accessible text (`value`+`label`) rendered this frame
/// (mirrors `now_playing.rs`/`queue_view.rs`'s own `rendered_texts`).
fn rendered_texts<B: OutputBackend, H: SourceHost>(
    ctx: &Context,
    controller: &mut PlaybackController<B, H>,
    screen: &mut ControlsScreen,
) -> Vec<String> {
    let update = render_update(ctx, controller, screen);
    update
        .nodes
        .iter()
        .flat_map(|(_, node)| [node.value(), node.label()])
        .filter_map(|text| text.map(str::to_string))
        .filter(|text| !text.is_empty())
        .collect()
}

/// The bounds of the `nth` (top-to-bottom) `role` node whose accessible
/// name equals `label` — several controls (every row's "Add binding",
/// "Reset to default"'s shared text on other pages, etc.) repeat the same
/// label once per row, so the row order disambiguates which one a click
/// lands on (mirrors `queue_view.rs`'s `nth_button_bounds`).
fn nth_role_bounds<B: OutputBackend, H: SourceHost>(
    ctx: &Context,
    controller: &mut PlaybackController<B, H>,
    screen: &mut ControlsScreen,
    role: Role,
    label: &str,
    nth: usize,
) -> Rect {
    let update = render_update(ctx, controller, screen);
    let mut matches: Vec<egui::accesskit::Rect> = update
        .nodes
        .iter()
        .filter(|(_, node)| node.role() == role && node.label() == Some(label))
        .filter_map(|(_, node)| node.bounds())
        .collect();
    matches.sort_by(|a, b| a.y0.partial_cmp(&b.y0).unwrap_or(std::cmp::Ordering::Equal));
    let bounds = matches.get(nth).unwrap_or_else(|| {
        panic!(
            "expected at least {} {role:?} node(s) labelled `{label}`, found {}",
            nth + 1,
            matches.len()
        )
    });
    Rect::from_min_max(
        Pos2::new(bounds.x0 as f32, bounds.y0 as f32),
        Pos2::new(bounds.x1 as f32, bounds.y1 as f32),
    )
}

fn nth_button_bounds<B: OutputBackend, H: SourceHost>(
    ctx: &Context,
    controller: &mut PlaybackController<B, H>,
    screen: &mut ControlsScreen,
    label: &str,
    nth: usize,
) -> Rect {
    nth_role_bounds(ctx, controller, screen, Role::Button, label, nth)
}

/// The bounds of the sole node of `role` this frame — used for the filter
/// box (`Role::TextInput`), which nothing else on this screen shares.
fn single_role_bounds<B: OutputBackend, H: SourceHost>(
    ctx: &Context,
    controller: &mut PlaybackController<B, H>,
    screen: &mut ControlsScreen,
    role: Role,
) -> Rect {
    let update = render_update(ctx, controller, screen);
    let matches: Vec<egui::accesskit::Rect> = update
        .nodes
        .iter()
        .filter(|(_, node)| node.role() == role)
        .filter_map(|(_, node)| node.bounds())
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one {role:?} node, found {}",
        matches.len()
    );
    let bounds = matches[0];
    Rect::from_min_max(
        Pos2::new(bounds.x0 as f32, bounds.y0 as f32),
        Pos2::new(bounds.x1 as f32, bounds.y1 as f32),
    )
}

/// Press then release the primary button at `pos`, in two separate frames
/// (mirrors `now_playing.rs`/`queue_view.rs`'s proven press/release
/// pattern — a single frame is not guaranteed to register `clicked()`).
fn click_at<B: OutputBackend, H: SourceHost>(
    ctx: &Context,
    controller: &mut PlaybackController<B, H>,
    screen: &mut ControlsScreen,
    pos: Pos2,
) {
    let mut press = default_input();
    press.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::default(),
    });
    let output = ctx.run_ui(press, |ui| controls::show(ui, controller, screen, None));
    output.drop_without_applying_deltas();

    let mut release = default_input();
    release.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::default(),
    });
    let output = ctx.run_ui(release, |ui| controls::show(ui, controller, screen, None));
    output.drop_without_applying_deltas();
}

/// Render one frame carrying `event` and return afterward — used to feed
/// the open capture control (or the "Reset all" confirm pair) a single
/// key press.
fn press_key<B: OutputBackend, H: SourceHost>(
    ctx: &Context,
    controller: &mut PlaybackController<B, H>,
    screen: &mut ControlsScreen,
    event: Event,
) {
    let mut input = default_input();
    input.events.push(event);
    let output = ctx.run_ui(input, |ui| controls::show(ui, controller, screen, None));
    output.drop_without_applying_deltas();
}

// -- T062: filter and full-catalog listing ---------------------------------

#[test]
fn filter_matches_label_and_category_case_insensitively() {
    let (mut controller, _dir) = fresh_controller("filter-label");
    let ctx = Context::default();
    ctx.enable_accesskit();

    let mut screen = ControlsScreen {
        filter: "PLAY/PAUSE".to_string(),
        ..Default::default()
    };
    let texts = rendered_texts(&ctx, &mut controller, &mut screen);
    assert!(
        texts.iter().any(|t| t == &tr("action-transport-toggle")),
        "an upper-case filter must still match the action's (mixed-case) label"
    );
    assert!(
        !texts.iter().any(|t| t == &tr("action-loop-toggle")),
        "a non-matching row's label must not render"
    );

    let (mut controller2, _dir2) = fresh_controller("filter-category");
    let mut screen2 = ControlsScreen {
        filter: "markers".to_string(),
        ..Default::default()
    };
    let texts2 = rendered_texts(&ctx, &mut controller2, &mut screen2);
    for action in HostAction::ALL {
        if action.category() == ActionCategory::Markers {
            assert!(
                texts2.iter().any(|t| t == &tr(action.label_key())),
                "a category-label match must show every row in that category (`{}` missing)",
                action.id()
            );
        }
    }
    assert!(
        !texts2.iter().any(|t| t == &tr("action-transport-play")),
        "a Transport row must not render when only the Markers category label matched"
    );
}

#[test]
fn filter_no_match_shows_line() {
    let (mut controller, _dir) = fresh_controller("filter-no-match");
    let ctx = Context::default();
    ctx.enable_accesskit();

    let mut screen = ControlsScreen {
        filter: "zzz-nomatch-zzz".to_string(),
        ..Default::default()
    };
    let texts = rendered_texts(&ctx, &mut controller, &mut screen);
    assert!(texts.iter().any(|t| t == &tr("controls-no-match")));
    for category in ActionCategory::ALL {
        assert!(
            !texts.iter().any(|t| t == &tr(category.label_key())),
            "no category heading may render when nothing in it matches"
        );
    }
}

#[test]
fn every_action_listed_grouped_by_category() {
    let (mut controller, _dir) = fresh_controller("every-action");
    let ctx = Context::default();
    ctx.enable_accesskit();

    let mut screen = ControlsScreen::default();
    let texts = rendered_texts(&ctx, &mut controller, &mut screen);

    for category in ActionCategory::ALL {
        assert!(
            texts.iter().any(|t| t == &tr(category.label_key())),
            "missing category heading `{}`",
            category.label_key()
        );
    }
    for action in HostAction::ALL {
        let label = tr(action.label_key());
        let expected = if controller.actions().is_enabled(action) {
            label
        } else {
            format!("{label} {}", tr("controls-inactive"))
        };
        assert!(
            texts.iter().any(|t| t == &expected),
            "missing action row `{}`",
            action.id()
        );
    }
    assert!(!texts.iter().any(|t| t == &tr("controls-no-match")));
}

// -- T063: capture mode -----------------------------------------------------

#[test]
fn capture_accepts_chord_and_adds_chip() {
    let (mut controller, _dir) = fresh_controller("capture-accept");
    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut screen = ControlsScreen::default();

    let action = HostAction::NavPlugins;
    let index = HostAction::ALL
        .iter()
        .position(|&a| a == action)
        .unwrap_or_else(|| unreachable!());

    let bounds = nth_button_bounds(
        &ctx,
        &mut controller,
        &mut screen,
        &tr("controls-add-binding"),
        index,
    );
    click_at(&ctx, &mut controller, &mut screen, bounds.center());
    assert_eq!(
        screen.capture,
        Some(ActionId::Host(action)),
        "clicking a row's Add binding must open capture for that action"
    );

    press_key(
        &ctx,
        &mut controller,
        &mut screen,
        key(EguiKey::K, Modifiers::NONE),
    );

    assert_eq!(
        screen.capture, None,
        "an accepted candidate must close capture"
    );
    assert_eq!(screen.capture_error, None);
    let chord = Chord::parse("K").unwrap_or_else(|_| unreachable!());
    assert!(
        controller.actions().bindings(action).contains(&chord),
        "the captured chord must be added to the action's bindings"
    );
}

#[test]
fn capture_accepts_space_enter_arrows_function_keys() {
    let accepted = [
        EguiKey::Space,
        EguiKey::Enter,
        EguiKey::ArrowLeft,
        EguiKey::ArrowRight,
        EguiKey::ArrowUp,
        EguiKey::ArrowDown,
        EguiKey::F5,
        EguiKey::Num3,
        EguiKey::Comma,
    ];
    for candidate in accepted {
        let event = key(candidate, Modifiers::NONE);
        assert!(
            CaptureRule::check(&event, false, &[]).is_ok(),
            "`{candidate:?}` must be accepted by capture (FR-007: every key but modifier-only/Tab/macOS-Control/duplicate)"
        );
    }
}

#[test]
fn capture_rejects_duplicate_tab_and_mac_control_inline() {
    // Duplicate: the exact chord an action already holds.
    let existing = vec![Chord::parse("Primary+1").unwrap_or_else(|_| unreachable!())];
    let duplicate = key(EguiKey::Num1, Modifiers::COMMAND);
    assert_eq!(
        CaptureRule::check(&duplicate, false, &existing),
        Err(CaptureReject::Duplicate)
    );

    // Tab / Shift+Tab: reserved for focus navigation.
    assert_eq!(
        CaptureRule::check(&key(EguiKey::Tab, Modifiers::NONE), false, &[]),
        Err(CaptureReject::TabReserved)
    );
    assert_eq!(
        CaptureRule::check(&key(EguiKey::Tab, Modifiers::SHIFT), false, &[]),
        Err(CaptureReject::TabReserved)
    );

    // macOS's physical Control: unbindable there, fine everywhere else
    // (Ctrl is just Primary off macOS).
    let control_chord = key(
        EguiKey::A,
        Modifiers {
            ctrl: true,
            ..Modifiers::default()
        },
    );
    assert_eq!(
        CaptureRule::check(&control_chord, true, &[]),
        Err(CaptureReject::MacControl)
    );
    assert!(CaptureRule::check(&control_chord, false, &[]).is_ok());

    // Lone modifier: the synthetic case of a resolved key name that is
    // itself a physical-modifier name (data-model.md §4.3's own note —
    // egui-winit never emits a real `Event::Key` for a bare modifier
    // press, so this is exercised directly rather than through the UI).
    let shift_left = KeyName::parse("ShiftLeft").unwrap_or_else(|| unreachable!());
    let shift_left_key = EguiKey::from_name(shift_left.as_str()).unwrap_or_else(|| unreachable!());
    let lone_modifier = key(
        shift_left_key,
        Modifiers {
            shift: true,
            ..Modifiers::default()
        },
    );
    assert_eq!(
        CaptureRule::check(&lone_modifier, false, &[]),
        Err(CaptureReject::LoneModifier)
    );
}

#[test]
fn capture_esc_and_focus_loss_cancel() {
    let (mut controller, _dir) = fresh_controller("capture-esc");
    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut screen = ControlsScreen::default();

    let action = HostAction::NavPlugins;
    let index = HostAction::ALL
        .iter()
        .position(|&a| a == action)
        .unwrap_or_else(|| unreachable!());

    // Esc cancels with no change.
    let bounds = nth_button_bounds(
        &ctx,
        &mut controller,
        &mut screen,
        &tr("controls-add-binding"),
        index,
    );
    click_at(&ctx, &mut controller, &mut screen, bounds.center());
    assert_eq!(screen.capture, Some(ActionId::Host(action)));
    let before = controller.actions().bindings(action).to_vec();

    press_key(
        &ctx,
        &mut controller,
        &mut screen,
        key(EguiKey::Escape, Modifiers::NONE),
    );
    assert_eq!(screen.capture, None, "Esc must cancel capture");
    assert_eq!(
        controller.actions().bindings(action),
        before.as_slice(),
        "Esc must not change any binding"
    );

    // Focus loss (click elsewhere — the filter box): cancels the same way.
    let (mut controller2, _dir2) = fresh_controller("capture-focus-loss");
    let ctx2 = Context::default();
    ctx2.enable_accesskit();
    let mut screen2 = ControlsScreen::default();

    let bounds2 = nth_button_bounds(
        &ctx2,
        &mut controller2,
        &mut screen2,
        &tr("controls-add-binding"),
        index,
    );
    click_at(&ctx2, &mut controller2, &mut screen2, bounds2.center());
    assert_eq!(screen2.capture, Some(ActionId::Host(action)));

    // A neutral frame lets the capture control's own `request_focus()`
    // actually take effect, so the *next* frame's `has_focus()` reflects
    // reality (contracts/ui-actions.md §5: "on any later frame").
    let output = ctx2.run_ui(default_input(), |ui| {
        controls::show(ui, &mut controller2, &mut screen2, None);
    });
    output.drop_without_applying_deltas();
    assert_eq!(
        screen2.capture,
        Some(ActionId::Host(action)),
        "sanity: still open after the settle frame"
    );

    let filter_bounds = single_role_bounds(&ctx2, &mut controller2, &mut screen2, Role::TextInput);
    click_at(
        &ctx2,
        &mut controller2,
        &mut screen2,
        filter_bounds.center(),
    );
    assert_eq!(
        screen2.capture, None,
        "clicking elsewhere must cancel capture with no change"
    );
}

// -- T064: chip remove, per-action reset, page-level two-step reset --------

#[test]
fn remove_chip_immediately_allows_zero_bindings() {
    let (mut controller, _dir) = fresh_controller("remove-chip");
    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut screen = ControlsScreen::default();

    let action = HostAction::ToggleLoop; // ships with exactly one default: "L".
    let chord = Chord::parse("L").unwrap_or_else(|_| unreachable!());
    let remove_label = tr_args(
        "controls-remove-binding",
        &[("binding", chord.display(platform_now()))],
    );

    let bounds = nth_button_bounds(&ctx, &mut controller, &mut screen, &remove_label, 0);
    click_at(&ctx, &mut controller, &mut screen, bounds.center());

    assert!(
        controller.actions().bindings(action).is_empty(),
        "removing the only chip must leave the action with zero bindings"
    );
}

#[test]
fn reset_action_restores_default_without_confirm() {
    let (mut controller, _dir) = fresh_controller("reset-action");
    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut screen = ControlsScreen::default();

    let action = HostAction::ToggleLoop;
    let extra = Chord::parse("K").unwrap_or_else(|_| unreachable!());
    controller
        .add_binding(action, extra)
        .unwrap_or_else(|_| unreachable!());
    assert_eq!(
        controller.actions().bindings(action).len(),
        2,
        "sanity: two bindings before reset"
    );

    let reset_label = tr_args(
        "controls-reset-action",
        &[("action", tr(action.label_key()))],
    );
    let bounds = nth_button_bounds(&ctx, &mut controller, &mut screen, &reset_label, 0);
    click_at(&ctx, &mut controller, &mut screen, bounds.center());

    let default = Chord::parse("L").unwrap_or_else(|_| unreachable!());
    assert_eq!(
        controller.actions().bindings(action),
        &[default][..],
        "Reset to default must restore exactly the shipped default, immediately, with no confirm step"
    );
}

#[test]
fn reset_all_two_step_confirm_cancel_esc_focus_loss() {
    let (mut controller, _dir) = fresh_controller("reset-all");
    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut screen = ControlsScreen::default();

    let action = HostAction::ToggleLoop;
    controller
        .add_binding(action, Chord::parse("K").unwrap_or_else(|_| unreachable!()))
        .unwrap_or_else(|_| unreachable!());
    let two_bindings = controller.actions().bindings(action).to_vec();
    assert_eq!(two_bindings.len(), 2);

    // Activating alone must not reset anything.
    let activate = |ctx: &Context,
                    controller: &mut PlaybackController<FakeBackend, SyntheticHost>,
                    screen: &mut ControlsScreen| {
        let bounds = nth_button_bounds(ctx, controller, screen, &tr("controls-reset-all"), 0);
        click_at(ctx, controller, screen, bounds.center());
    };
    activate(&ctx, &mut controller, &mut screen);
    assert!(screen.reset_all_confirm);
    assert_eq!(
        controller.actions().bindings(action),
        two_bindings.as_slice()
    );

    // Cancel: no change, back to the plain button.
    let cancel_bounds = nth_button_bounds(
        &ctx,
        &mut controller,
        &mut screen,
        &tr("controls-cancel"),
        0,
    );
    click_at(&ctx, &mut controller, &mut screen, cancel_bounds.center());
    assert!(!screen.reset_all_confirm);
    assert_eq!(
        controller.actions().bindings(action),
        two_bindings.as_slice()
    );

    // Esc: same effect as Cancel.
    activate(&ctx, &mut controller, &mut screen);
    assert!(screen.reset_all_confirm);
    press_key(
        &ctx,
        &mut controller,
        &mut screen,
        key(EguiKey::Escape, Modifiers::NONE),
    );
    assert!(!screen.reset_all_confirm, "Esc must cancel the confirm");
    assert_eq!(
        controller.actions().bindings(action),
        two_bindings.as_slice()
    );

    // Focus loss (click elsewhere — the filter box): cancels too.
    activate(&ctx, &mut controller, &mut screen);
    assert!(screen.reset_all_confirm);
    let output = ctx.run_ui(default_input(), |ui| {
        controls::show(ui, &mut controller, &mut screen, None);
    });
    output.drop_without_applying_deltas();
    let filter_bounds = single_role_bounds(&ctx, &mut controller, &mut screen, Role::TextInput);
    click_at(&ctx, &mut controller, &mut screen, filter_bounds.center());
    assert!(
        !screen.reset_all_confirm,
        "losing focus must cancel the confirm with no change"
    );
    assert_eq!(
        controller.actions().bindings(action),
        two_bindings.as_slice()
    );

    // Confirm: actually resets every binding.
    activate(&ctx, &mut controller, &mut screen);
    let confirm_bounds = nth_button_bounds(
        &ctx,
        &mut controller,
        &mut screen,
        &tr("controls-confirm"),
        0,
    );
    click_at(&ctx, &mut controller, &mut screen, confirm_bounds.center());
    assert!(!screen.reset_all_confirm);
    assert_eq!(
        controller.actions().bindings(action),
        &[Chord::parse("L").unwrap_or_else(|_| unreachable!())][..],
        "Confirm must reset every binding to its shipped default"
    );
}

// -- T065: disabled rows -----------------------------------------------------

#[test]
fn disabled_rows_greyed_rebindable_and_never_fire() {
    let (mut controller, _dir) = fresh_controller("disabled-rows");
    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut screen = ControlsScreen::default();

    // 008 flips the two Effects actions' shipped default to enabled
    // (T049); disable one explicitly to exercise this row's disabled
    // rendering/rebinding/never-fires behaviour.
    let action = HostAction::TempoStepUp;
    controller.set_action_enabled(action, false);
    assert!(!controller.actions().is_enabled(action));

    let texts = rendered_texts(&ctx, &mut controller, &mut screen);
    let expected_label = format!("{} {}", tr(action.label_key()), tr("controls-inactive"));
    assert!(
        texts.iter().any(|t| t == &expected_label),
        "a disabled row's label must show the `(inactive)` suffix"
    );

    // Rebindable while disabled.
    let extra = Chord::parse("F6").unwrap_or_else(|_| unreachable!());
    controller
        .add_binding(action, extra)
        .unwrap_or_else(|_| unreachable!());
    assert!(controller.actions().bindings(action).contains(&extra));

    // Never fires, even with its scope live.
    let scope = ScopeState {
        now_playing_shown: true,
        marker_focused: false,
    };
    assert_eq!(
        controller.actions().resolve(extra, &scope),
        None,
        "a disabled action's binding must never resolve"
    );
}

// -- T066: platform display --------------------------------------------------

#[test]
fn chips_display_platform_glyphs() {
    let (mut controller, _dir) = fresh_controller("chip-glyphs");
    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut screen = ControlsScreen::default();

    let texts = rendered_texts(&ctx, &mut controller, &mut screen);
    let chord = Chord::parse("Primary+1").unwrap_or_else(|_| unreachable!());
    let expected = tr_args(
        "controls-binding-chip",
        &[("binding", chord.display(platform_now()))],
    );
    assert!(
        texts.iter().any(|t| t == &expected),
        "a binding chip must render via `Chord::display` for the detected platform"
    );
}

// -- T073/T075: US3 conflict surfacing (contracts/ui-actions.md §4/§5) -----

/// Rebinding "toggle loop" onto "play/pause toggle"'s own key flags both
/// rows with `⚠` + `controls-conflict-with { $other }`, each naming the
/// other action (US3 AS1-AS3, contracts/ui-actions.md §4).
#[test]
fn conflict_flag_shows_on_both_rows() {
    let (mut controller, _dir) = fresh_controller("conflict-both-rows");
    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut screen = ControlsScreen::default();

    let space = Chord::parse("Space").unwrap_or_else(|_| unreachable!()); // TogglePlayPause's default.
    controller
        .add_binding(HostAction::ToggleLoop, space)
        .unwrap_or_else(|_| unreachable!());
    assert!(
        controller
            .actions()
            .is_conflicting(HostAction::TogglePlayPause, space)
            && controller
                .actions()
                .is_conflicting(HostAction::ToggleLoop, space),
        "sanity: sharing Space must flag both actions"
    );

    let texts = rendered_texts(&ctx, &mut controller, &mut screen);
    let flag_on_toggle = format!(
        "⚠ {}",
        tr_args(
            "controls-conflict-with-host",
            &[("other", tr(HostAction::ToggleLoop.label_key()))]
        )
    );
    let flag_on_loop = format!(
        "⚠ {}",
        tr_args(
            "controls-conflict-with-host",
            &[("other", tr(HostAction::TogglePlayPause.label_key()))]
        )
    );
    assert!(
        texts.iter().any(|t| t == &flag_on_toggle),
        "the play/pause row's Space chip must name `toggle loop` as its conflict partner"
    );
    assert!(
        texts.iter().any(|t| t == &flag_on_loop),
        "the toggle-loop row's Space chip must name `play/pause` as its conflict partner"
    );
}

/// Capturing a chord that another enabled action already holds accepts it
/// immediately (FR-007) and, in that same frame, names the conflict
/// partner inline (contracts/ui-actions.md §5 step 2's last bullet).
#[test]
fn capture_of_conflicting_chord_flags_both_and_names_partner() {
    let (mut controller, _dir) = fresh_controller("conflict-capture");
    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut screen = ControlsScreen::default();

    let action = HostAction::ToggleLoop;
    let index = HostAction::ALL
        .iter()
        .position(|&a| a == action)
        .unwrap_or_else(|| unreachable!());

    let bounds = nth_button_bounds(
        &ctx,
        &mut controller,
        &mut screen,
        &tr("controls-add-binding"),
        index,
    );
    click_at(&ctx, &mut controller, &mut screen, bounds.center());
    assert_eq!(screen.capture, Some(ActionId::Host(action)));

    // Space: TogglePlayPause's default binding, not one `ToggleLoop`
    // already holds — CaptureRule accepts it (only a *duplicate on this
    // action* is rejected inline; a cross-action conflict is not).
    let mut input = default_input();
    input.events.push(key(EguiKey::Space, Modifiers::NONE));
    let mut output = ctx.run_ui(input, |ui| {
        controls::show(ui, &mut controller, &mut screen, None)
    });
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    output.drop_without_applying_deltas();

    assert_eq!(
        screen.capture, None,
        "a cross-action conflict is accepted, not rejected inline"
    );
    let space = Chord::parse("Space").unwrap_or_else(|_| unreachable!());
    assert!(
        controller.actions().bindings(action).contains(&space),
        "the conflicting chord must still be added to the action's bindings"
    );
    assert_eq!(
        controller.actions().conflict_partner(action, space),
        Some(ActionId::Host(HostAction::TogglePlayPause))
    );

    let texts: Vec<String> = update
        .nodes
        .iter()
        .flat_map(|(_, node)| [node.value(), node.label()])
        .filter_map(|text| text.map(str::to_string))
        .filter(|text| !text.is_empty())
        .collect();
    let expected = format!(
        "⚠ {}",
        tr_args(
            "controls-conflict-with-host",
            &[("other", tr(HostAction::TogglePlayPause.label_key()))]
        )
    );
    assert!(
        texts.iter().any(|t| t.contains(&expected)),
        "the conflict partner's name must appear inline the same frame capture closes"
    );
}

// -- T066 (011-plugin-ui-contributions US2, contracts/action-registry-
// plugins.md G15): a plugin's own action group, tier-aware conflict text,
// and a suspended plugin's greyed row — driven through the real
// `org.modplayer.fixture.ui-shortcuts`/`ui-panel` fixtures (mirrors
// `plugin_panels.rs`'s own fixture harness). ---------------------------

const UI_SHORTCUTS: &str = "org.modplayer.fixture.ui-shortcuts";
const UI_PANEL: &str = "org.modplayer.fixture.ui-panel";

/// `MODPLAYER_PLUGIN_FIXTURES`/`MODPLAYER_PLUGIN_STATE_DIR`/
/// `MODPLAYER_TRACK_STATE_DIR` are process-global (mirrors `plugin_panels.
/// rs`'s own `PLUGIN_ENV_LOCK`).
static PLUGIN_ENV_LOCK: Mutex<()> = Mutex::new(());

fn fixture_controller(
    label: &str,
) -> (
    PlaybackController<FakeBackend, SyntheticHost>,
    TempDir,
    TempDir,
    TempDir,
) {
    let (store, dir) = fresh_store(label);
    let plugin_state_dir = TempDir::new(&format!("{label}-plugin-state"));
    let track_state_dir = TempDir::new(&format!("{label}-track-state"));
    let controller = {
        let _guard = PLUGIN_ENV_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        // Safety: narrowly scopes each mutation to the one synchronous
        // read `PlaybackController::new` makes of it, serialized against
        // every other test in this binary via the lock above.
        unsafe {
            std::env::set_var("MODPLAYER_PLUGIN_FIXTURES", "1");
            std::env::set_var("MODPLAYER_PLUGIN_STATE_DIR", plugin_state_dir.path());
            std::env::set_var("MODPLAYER_TRACK_STATE_DIR", track_state_dir.path());
        }
        let controller =
            PlaybackController::new(FakeBackend::new(vec![]), SyntheticHost::new(44_100), store);
        unsafe {
            std::env::remove_var("MODPLAYER_PLUGIN_FIXTURES");
            std::env::remove_var("MODPLAYER_PLUGIN_STATE_DIR");
            std::env::remove_var("MODPLAYER_TRACK_STATE_DIR");
        }
        controller
    };
    (controller, dir, plugin_state_dir, track_state_dir)
}

fn fixture_id(
    controller: &mut PlaybackController<FakeBackend, SyntheticHost>,
    identifier: &str,
) -> PluginId {
    controller
        .plugins_mut()
        .records()
        .iter()
        .find(|r| r.identifier.as_str() == identifier)
        .map(|r| r.id)
        .unwrap_or_else(|| unreachable!("fixture '{identifier}' must be discovered"))
}

fn pump_until(
    controller: &mut PlaybackController<FakeBackend, SyntheticHost>,
    timeout: Duration,
    mut done: impl FnMut(&mut PlaybackController<FakeBackend, SyntheticHost>) -> bool,
) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        controller.tick();
        if done(controller) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn wait_active(
    controller: &mut PlaybackController<FakeBackend, SyntheticHost>,
    id: PluginId,
) -> bool {
    pump_until(controller, Duration::from_secs(2), |c| {
        matches!(
            c.plugins_mut().record(id).map(|r| &r.lifecycle),
            Some(Lifecycle::Active)
        )
    })
}

fn wait_suspended(
    controller: &mut PlaybackController<FakeBackend, SyntheticHost>,
    id: PluginId,
) -> bool {
    pump_until(controller, Duration::from_secs(2), |c| {
        matches!(
            c.plugins_mut().record(id).map(|r| &r.lifecycle),
            Some(Lifecycle::Suspended { .. })
        )
    })
}

/// `fixture_controller` only discovers records — every plugin must be
/// `spawn`ed individually (mirrors `plugin_panels.rs::launch_ui_panel`).
fn spawn_and_wait_active(
    controller: &mut PlaybackController<FakeBackend, SyntheticHost>,
    id: PluginId,
) -> bool {
    let shared = Arc::clone(controller.shared());
    controller.plugins_mut().spawn(id, &shared);
    wait_active(controller, id)
}

fn identifier_of(
    controller: &mut PlaybackController<FakeBackend, SyntheticHost>,
    id: PluginId,
) -> String {
    controller
        .plugins_mut()
        .record(id)
        .unwrap_or_else(|| unreachable!())
        .identifier
        .to_string()
}

/// A plugin's own group heading renders (its resolved display name) and
/// its actions render under it with their own literal labels — never
/// translated through Fluent, unlike a host row (contracts/
/// action-registry-plugins.md G15 "a plugin row carries `label:
/// RowLabel::Literal(resolved string)`").
#[test]
fn plugin_group_rendered() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller("plugin-group");
    let id = fixture_id(&mut controller, UI_SHORTCUTS);
    assert!(spawn_and_wait_active(&mut controller, id));
    let identifier = identifier_of(&mut controller, id);
    let action_id =
        PluginActionId::parse(&format!("{identifier}.take_over")).unwrap_or_else(|| unreachable!());
    assert!(pump_until(&mut controller, Duration::from_secs(2), |c| {
        c.actions().plugin_action_registered(&action_id)
    }));

    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut screen = ControlsScreen::default();
    let texts = rendered_texts(&ctx, &mut controller, &mut screen);

    let heading = tr_args(
        "controls-plugin-group",
        &[("plugin", "UI Shortcuts fixture".to_string())],
    );
    assert!(
        texts.iter().any(|t| t == &heading),
        "the owning plugin's own display name must head its action group"
    );
    assert!(
        texts
            .iter()
            .any(|t| t == "Take over (collides with host L)"),
        "a plugin action's own literal label must render, not a Fluent lookup"
    );
}

/// The host-vs-plugin conflict baked into the `ui-shortcuts` fixture
/// itself (`take_over`'s default `L`, colliding with the host's own
/// `ToggleLoop`, M5) renders the tier-aware `controls-conflict-with-host`
/// message on the plugin's own chip (contracts/action-registry-plugins.md
/// G15 "conflict text names the partner's tier").
#[test]
fn tier_conflict_text() {
    let (mut controller, _dir, _psd, _tsd) = fixture_controller("tier-conflict");
    let id = fixture_id(&mut controller, UI_SHORTCUTS);
    assert!(spawn_and_wait_active(&mut controller, id));
    let identifier = identifier_of(&mut controller, id);
    let action_id =
        PluginActionId::parse(&format!("{identifier}.take_over")).unwrap_or_else(|| unreachable!());
    let l = Chord::parse("L").unwrap_or_else(|_| unreachable!());
    assert!(pump_until(&mut controller, Duration::from_secs(2), |c| {
        c.actions()
            .is_conflicting(ActionId::Plugin(action_id.clone()), l)
    }));

    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut screen = ControlsScreen::default();
    let texts = rendered_texts(&ctx, &mut controller, &mut screen);

    let expected = format!(
        "⚠ {}",
        tr_args(
            "controls-conflict-with-host",
            &[("other", tr(HostAction::ToggleLoop.label_key()))]
        )
    );
    assert!(
        texts.iter().any(|t| t == &expected),
        "the plugin action's flagged `L` chip must name the host's tier, got: {texts:?}"
    );
}

/// A row whose owning plugin is `Suspended` (not `Active`) renders greyed
/// with the same `(inactive)` suffix a disabled host row uses (contracts/
/// action-registry-plugins.md G15 "a row whose plugin is not Active
/// renders greyed").
#[test]
fn greyed_when_suspended() {
    use modplayer_capability_gateway::budgets::Budgets;

    // Not the plain `fixture_controller` + auto-spawn: the running
    // plugin thread captures its `Budgets` once, at spawn (mirrors
    // `plugin_panels.rs::placeholder_on_suspend`), so the 1 ms share
    // override must land *before* `spawn`.
    let (mut controller, _dir, _psd, _tsd) = fixture_controller("greyed-suspended");
    let id = fixture_id(&mut controller, UI_PANEL);
    if let Some(record) = controller.plugins_mut().record_mut(id) {
        record.budgets = Budgets {
            share: Duration::from_millis(1),
            ..Budgets::DEFAULT
        };
    }
    let shared = Arc::clone(controller.shared());
    controller.plugins_mut().spawn(id, &shared);
    assert!(
        wait_active(&mut controller, id),
        "the ui-panel fixture must reach Active on its own"
    );

    let identifier = identifier_of(&mut controller, id);
    let action_id =
        PluginActionId::parse(&format!("{identifier}.take_over")).unwrap_or_else(|| unreachable!());
    assert!(pump_until(&mut controller, Duration::from_secs(2), |c| {
        c.actions().plugin_action_registered(&action_id)
    }));

    // A 1 ms share suspends the fixture's own "hang" busy-loop probe the
    // moment it runs.
    let panel =
        modplayer_capability_gateway::ui::UiId::parse("main").unwrap_or_else(|| unreachable!());
    let widget =
        modplayer_capability_gateway::ui::UiId::parse("hang").unwrap_or_else(|| unreachable!());
    controller.plugin_panel_interaction(id, &panel, &widget, WidgetValue::Bool(true));
    assert!(
        wait_suspended(&mut controller, id),
        "the hang probe must suspend the fixture under a 1ms share"
    );

    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut screen = ControlsScreen::default();
    let texts = rendered_texts(&ctx, &mut controller, &mut screen);

    let expected = format!("Take over transport {}", tr("controls-inactive"));
    assert!(
        texts.iter().any(|t| t == &expected),
        "a suspended plugin's own action row must render the `(inactive)` suffix, got: {texts:?}"
    );
}

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

/// 014-design-tokens-and-type-scale (US2, T021-T023/T030/T032/T033): a
/// bare `Context::default()` has none of the token `Style`'s
/// `Name("display")`/`Name("section")` text styles installed, which every
/// screen this suite renders now reaches — panicking on layout otherwise.
/// Install them once, exactly as `App::new`/`App::update` do, so this
/// suite exercises the same `Style` the real app runs under.
fn fresh_ctx() -> Context {
    let ctx = Context::default();
    modplayer_ui::theme::apply_tokens(&ctx);
    ctx
}

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
    let ctx = fresh_ctx();
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

/// Spoken bindings of `action`, as its row element's accessible name
/// lists them (`controls-row`).
fn spoken_bindings<B: OutputBackend, H: SourceHost>(
    controller: &PlaybackController<B, H>,
    action: HostAction,
) -> String {
    let chords = controller.actions().bindings(action);
    if chords.is_empty() {
        return tr("shortcuts-not-bound");
    }
    chords
        .iter()
        .map(|&chord| modplayer_ui::key_label::KeyLabel::new(chord, platform_now()).spoken)
        .collect::<Vec<_>>()
        .join(", ")
}

/// The bounds of `action`'s row element (its focusable name label).
fn row_bounds<B: OutputBackend, H: SourceHost>(
    ctx: &Context,
    controller: &mut PlaybackController<B, H>,
    screen: &mut ControlsScreen,
    action: HostAction,
) -> Rect {
    let name = tr_args(
        "controls-row",
        &[
            ("action", tr(action.label_key())),
            ("bindings", spoken_bindings(controller, action)),
        ],
    );
    let update = render_update(ctx, controller, screen);
    let bounds = update
        .nodes
        .iter()
        .find(|(_, node)| node.label() == Some(name.as_str()))
        .and_then(|(_, node)| node.bounds())
        .unwrap_or_else(|| panic!("no row element named `{name}`"));
    Rect::from_min_max(
        Pos2::new(bounds.x0 as f32, bounds.y0 as f32),
        Pos2::new(bounds.x1 as f32, bounds.y1 as f32),
    )
}

/// Move the pointer onto `action`'s row and render until its controls are
/// revealed (the pointer stays there for later frames).
fn hover_row<B: OutputBackend, H: SourceHost>(
    ctx: &Context,
    controller: &mut PlaybackController<B, H>,
    screen: &mut ControlsScreen,
    action: HostAction,
) {
    let pos = row_bounds(ctx, controller, screen, action).center();
    for _ in 0..2 {
        let mut input = default_input();
        input.events.push(Event::PointerMoved(pos));
        let output = ctx.run_ui(input, |ui| controls::show(ui, controller, screen, None));
        output.drop_without_applying_deltas();
    }
}

// -- T062: filter and full-catalog listing ---------------------------------

#[test]
fn filter_matches_label_and_category_case_insensitively() {
    let (mut controller, _dir) = fresh_controller("filter-label");
    let ctx = fresh_ctx();
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
    let ctx = fresh_ctx();
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
    let ctx = fresh_ctx();
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
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut screen = ControlsScreen::default();

    let action = HostAction::NavPlugins;

    hover_row(&ctx, &mut controller, &mut screen, action);
    let bounds = nth_button_bounds(
        &ctx,
        &mut controller,
        &mut screen,
        &tr("controls-add-binding"),
        0,
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
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut screen = ControlsScreen::default();

    let action = HostAction::NavPlugins;

    // Esc cancels with no change.
    hover_row(&ctx, &mut controller, &mut screen, action);
    let bounds = nth_button_bounds(
        &ctx,
        &mut controller,
        &mut screen,
        &tr("controls-add-binding"),
        0,
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
    let ctx2 = fresh_ctx();
    ctx2.enable_accesskit();
    let mut screen2 = ControlsScreen::default();

    hover_row(&ctx2, &mut controller2, &mut screen2, action);
    let bounds2 = nth_button_bounds(
        &ctx2,
        &mut controller2,
        &mut screen2,
        &tr("controls-add-binding"),
        0,
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
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut screen = ControlsScreen::default();

    let action = HostAction::ToggleLoop; // ships with exactly one default: "L".
    let chord = Chord::parse("L").unwrap_or_else(|_| unreachable!());
    let remove_label = tr_args(
        "controls-remove-binding",
        &[("binding", chord.display(platform_now()))],
    );

    hover_row(&ctx, &mut controller, &mut screen, action);
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
    let ctx = fresh_ctx();
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
    hover_row(&ctx, &mut controller, &mut screen, action);
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
    let ctx = fresh_ctx();
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
    let ctx = fresh_ctx();
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
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut screen = ControlsScreen::default();

    let texts = rendered_texts(&ctx, &mut controller, &mut screen);
    let chord = Chord::parse("Primary+1").unwrap_or_else(|_| unreachable!());
    let expected = tr_args(
        "controls-binding-chip",
        &[(
            "binding",
            modplayer_ui::key_label::KeyLabel::new(chord, platform_now()).spoken,
        )],
    );
    assert!(
        texts.iter().any(|t| t == &expected),
        "a binding chip's accessible name must use `KeyLabel::spoken` for the detected platform (C8)"
    );
}

// -- T073/T075: US3 conflict surfacing (contracts/ui-actions.md §4/§5) -----

/// Rebinding "toggle loop" onto "play/pause toggle"'s own key flags both
/// rows with `⚠` + `controls-conflict-with { $other }`, each naming the
/// other action (US3 AS1-AS3, contracts/ui-actions.md §4).
#[test]
fn conflict_flag_shows_on_both_rows() {
    let (mut controller, _dir) = fresh_controller("conflict-both-rows");
    let ctx = fresh_ctx();
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
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut screen = ControlsScreen::default();

    let action = HostAction::ToggleLoop;

    hover_row(&ctx, &mut controller, &mut screen, action);
    let bounds = nth_button_bounds(
        &ctx,
        &mut controller,
        &mut screen,
        &tr("controls-add-binding"),
        0,
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

    let ctx = fresh_ctx();
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

    let ctx = fresh_ctx();
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

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut screen = ControlsScreen::default();
    let texts = rendered_texts(&ctx, &mut controller, &mut screen);

    let expected = format!("Take over transport {}", tr("controls-inactive"));
    assert!(
        texts.iter().any(|t| t == &expected),
        "a suspended plugin's own action row must render the `(inactive)` suffix, got: {texts:?}"
    );
}

// ---------------------------------------------------------------------
// 029-keyboard-shortcut-reference (SC-005 baseline, research R12)
// ---------------------------------------------------------------------

/// Controls-page content height (px) at 1280×800, full host catalog,
/// default bindings, empty filter, measured on the PRE-029 layout.
/// US1's `controls_height_at_most_60_percent_of_baseline` (C17) asserts
/// against this; do not re-measure after the layout changes.
// Measured on commit 72b0019 (pre-029 layout): 1474.125.
const PRE_029_CONTROLS_HEIGHT: f32 = 1474.125;

/// Draw `controls::show` inside a `ScrollArea` at 1280×800 and return the
/// scroll content height (`content_size.y`).
fn measure_controls_content_height() -> f32 {
    let ctx = fresh_ctx();
    let (mut controller, _dir) = fresh_controller("height-baseline");
    let mut screen = ControlsScreen::default();
    let input = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1280.0, 800.0))),
        ..Default::default()
    };
    let mut height = 0.0_f32;
    // Two frames so layout settles.
    for _ in 0..2 {
        let output = ctx.run_ui(input.clone(), |ui| {
            let out = egui::ScrollArea::vertical().show(ui, |ui| {
                controls::show(ui, &mut controller, &mut screen, None);
            });
            height = out.content_size.y;
        });
        output.drop_without_applying_deltas();
    }
    height
}

#[test]
fn baseline_controls_height_measurement() {
    let height = measure_controls_content_height();
    assert!(height > 0.0, "controls page must have content");
}

// ---------------------------------------------------------------------
// 029-keyboard-shortcut-reference, US1 (contracts C1-C13, C17)
// ---------------------------------------------------------------------

fn sized_input(width: f32) -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(width, 4000.0))),
        ..Default::default()
    }
}

/// One frame at `width`, returning the AccessKit update.
fn render_at<B: OutputBackend, H: SourceHost>(
    ctx: &Context,
    controller: &mut PlaybackController<B, H>,
    screen: &mut ControlsScreen,
    width: f32,
) -> TreeUpdate {
    let mut output = ctx.run_ui(sized_input(width), |ui| {
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

fn rect_of(bounds: egui::accesskit::Rect) -> Rect {
    Rect::from_min_max(
        Pos2::new(bounds.x0 as f32, bounds.y0 as f32),
        Pos2::new(bounds.x1 as f32, bounds.y1 as f32),
    )
}

/// Bounds of every node (any role) whose accessible name equals `label`.
fn bounds_labelled(update: &TreeUpdate, label: &str) -> Vec<Rect> {
    update
        .nodes
        .iter()
        .filter(|(_, node)| node.label() == Some(label))
        .filter_map(|(_, node)| node.bounds())
        .map(rect_of)
        .collect()
}

fn host_headings() -> Vec<String> {
    ActionCategory::ALL
        .iter()
        .map(|c| tr(c.label_key()))
        .collect()
}

/// Distinct left edges (rounded) of the host group headings.
fn heading_columns(update: &TreeUpdate) -> Vec<i32> {
    let mut xs: Vec<i32> = host_headings()
        .iter()
        .flat_map(|h| bounds_labelled(update, h))
        .map(|r| r.min.x.round() as i32)
        .collect();
    xs.sort_unstable();
    xs.dedup();
    xs
}

fn pointer_to<B: OutputBackend, H: SourceHost>(
    ctx: &Context,
    controller: &mut PlaybackController<B, H>,
    screen: &mut ControlsScreen,
    pos: Pos2,
) {
    for _ in 0..2 {
        let mut input = default_input();
        input.events.push(Event::PointerMoved(pos));
        let output = ctx.run_ui(input, |ui| controls::show(ui, controller, screen, None));
        output.drop_without_applying_deltas();
    }
}

fn has_button<B: OutputBackend, H: SourceHost>(
    ctx: &Context,
    controller: &mut PlaybackController<B, H>,
    screen: &mut ControlsScreen,
    label: &str,
) -> bool {
    let update = render_update(ctx, controller, screen);
    update
        .nodes
        .iter()
        .any(|(_, n)| n.role() == Role::Button && n.label() == Some(label))
}

// -- C1-C5: layout ----------------------------------------------------------

#[test]
fn layout_order_filter_and_reset_all_above_columns() {
    let (mut controller, _dir) = fresh_controller("layout-order");
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut screen = ControlsScreen::default();
    let _ = render_at(&ctx, &mut controller, &mut screen, 1280.0);
    let update = render_at(&ctx, &mut controller, &mut screen, 1280.0);

    let filter = update
        .nodes
        .iter()
        .find(|(_, n)| n.role() == Role::TextInput)
        .and_then(|(_, n)| n.bounds())
        .map(rect_of)
        .expect("filter box");
    let reset_all = bounds_labelled(&update, &tr("controls-reset-all"));
    let reset_all = reset_all.first().expect("Reset all button");
    let first_heading_y = host_headings()
        .iter()
        .flat_map(|h| bounds_labelled(&update, h))
        .map(|r| r.min.y)
        .fold(f32::INFINITY, f32::min);
    assert!(
        filter.max.y <= reset_all.min.y + 1.0,
        "filter above Reset all"
    );
    assert!(reset_all.max.y <= first_heading_y, "Reset all above groups");
    assert!(filter.max.y <= first_heading_y, "filter above groups");
}

#[test]
fn groups_follow_catalog_then_plugin_order() {
    let (mut controller, _dir) = fresh_controller("group-order");
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut screen = ControlsScreen::default();
    // One column, so top-to-bottom is the group order.
    let _ = render_at(&ctx, &mut controller, &mut screen, 600.0);
    let update = render_at(&ctx, &mut controller, &mut screen, 600.0);

    let mut ys: Vec<(f32, String)> = host_headings()
        .into_iter()
        .filter_map(|h| {
            bounds_labelled(&update, &h)
                .first()
                .map(|r| (r.min.y, h.clone()))
        })
        .collect();
    ys.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let order: Vec<String> = ys.into_iter().map(|(_, h)| h).collect();
    assert_eq!(order, host_headings());

    // Plugin groups follow the host groups in the snapshot itself.
    let groups = modplayer_ui::shortcut_groups::snapshot(controller.actions());
    let first_plugin = groups
        .iter()
        .position(|g| matches!(g.group, modplayer_ui::shortcut_groups::RowGroup::Plugin(_)));
    if let Some(i) = first_plugin {
        assert!(
            groups[i..]
                .iter()
                .all(|g| matches!(g.group, modplayer_ui::shortcut_groups::RowGroup::Plugin(_)))
        );
    }
}

#[test]
fn two_columns_at_or_above_720() {
    let (mut controller, _dir) = fresh_controller("two-cols");
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut screen = ControlsScreen::default();
    let _ = render_at(&ctx, &mut controller, &mut screen, 1280.0);
    let update = render_at(&ctx, &mut controller, &mut screen, 1280.0);
    assert_eq!(
        heading_columns(&update).len(),
        2,
        "headings must sit in exactly two columns at 1280 px"
    );
}

#[test]
fn one_column_below_720() {
    let (mut controller, _dir) = fresh_controller("one-col");
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut screen = ControlsScreen::default();
    let _ = render_at(&ctx, &mut controller, &mut screen, 700.0);
    let update = render_at(&ctx, &mut controller, &mut screen, 700.0);
    assert_eq!(
        heading_columns(&update).len(),
        1,
        "headings must share one column at 700 px"
    );
}

/// A filter that leaves a single group puts it in the left column, not
/// alone on the right (found in the T051 manual walk).
#[test]
fn lone_filtered_group_sits_in_left_column() {
    let (mut controller, _dir) = fresh_controller("lone-group-left");
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut screen = ControlsScreen::default();
    let _ = render_at(&ctx, &mut controller, &mut screen, 1280.0);
    let unfiltered = heading_columns(&render_at(&ctx, &mut controller, &mut screen, 1280.0));
    assert_eq!(unfiltered.len(), 2, "sanity: two columns at 1280 px");

    screen.filter = "space".to_owned();
    let _ = render_at(&ctx, &mut controller, &mut screen, 1280.0);
    let filtered = heading_columns(&render_at(&ctx, &mut controller, &mut screen, 1280.0));
    assert_eq!(
        filtered,
        vec![unfiltered[0]],
        "the only matching group must start at the left column's origin"
    );
}

/// The groups keep clear of the page's floating scroll bar: nothing is
/// laid out in the scroll-bar gutter at the right edge (found in the T051
/// manual walk: right-column conflict notes ran under the bar).
#[test]
fn groups_keep_clear_of_scroll_bar_gutter() {
    let (mut controller, _dir) = fresh_controller("scroll-gutter");
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut screen = ControlsScreen::default();
    let width = 760.0;
    let _ = render_at(&ctx, &mut controller, &mut screen, width);
    let update = render_at(&ctx, &mut controller, &mut screen, width);
    let scroll = ctx.global_style().spacing.scroll;
    let limit = width - scroll.bar_width - scroll.bar_outer_margin;
    let rightmost = update
        .nodes
        .iter()
        .filter_map(|(_, node)| node.bounds())
        // Skip the root (the whole screen, starting at x = 0).
        .filter(|b| b.x0 > 0.0)
        .map(|b| b.x1 as f32)
        .fold(0.0_f32, f32::max);
    assert!(
        rightmost <= limit + 0.5,
        "content reaches x = {rightmost}, past the scroll-bar gutter at {limit}"
    );
}

fn many_bindings_wrap_without_elision_body() {
    let (mut controller, _dir) = fresh_controller("many-bindings");
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut screen = ControlsScreen::default();
    let action = HostAction::ToggleLoop;
    for extra in ["K", "J", "M", "N"] {
        let chord = Chord::parse(extra).unwrap_or_else(|_| unreachable!());
        controller
            .add_binding(action, chord)
            .unwrap_or_else(|_| unreachable!());
    }
    assert_eq!(controller.actions().bindings(action).len(), 5);

    let _ = render_at(&ctx, &mut controller, &mut screen, 700.0);
    let update = render_at(&ctx, &mut controller, &mut screen, 700.0);
    let mut rects = Vec::new();
    for &chord in controller.actions().bindings(action) {
        let spoken = modplayer_ui::key_label::KeyLabel::new(chord, platform_now()).spoken;
        let name = tr_args("controls-binding-chip", &[("binding", spoken)]);
        let found = bounds_labelled(&update, &name);
        assert!(!found.is_empty(), "chip `{name}` must be rendered");
        rects.extend(found.into_iter().take(1));
    }
    for (i, a) in rects.iter().enumerate() {
        assert!(
            a.max.x <= 700.0,
            "chip {i} must stay inside the page: {a:?}"
        );
        assert!(a.width() > 4.0 && a.height() > 4.0, "chip {i} not clipped");
        for b in &rects[i + 1..] {
            assert!(!a.intersects(*b), "chips must not overlap: {a:?} {b:?}");
        }
    }
}

#[test]
fn many_bindings_wrap_without_elision() {
    many_bindings_wrap_without_elision_body();
}

// -- C6-C9: row anatomy at rest ------------------------------------------

#[test]
fn row_at_rest_shows_name_chips_conflict_only() {
    let (mut controller, _dir) = fresh_controller("row-rest");
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut screen = ControlsScreen::default();
    let update = render_update(&ctx, &mut controller, &mut screen);
    let _ = update;
    let update = render_update(&ctx, &mut controller, &mut screen);

    let action = HostAction::ToggleLoop;
    let name = tr(action.label_key());
    assert!(
        update
            .nodes
            .iter()
            .any(|(_, n)| n.label().is_some_and(|l| l.contains(&name))),
        "the action name must be rendered"
    );
    let chord = Chord::parse("L").unwrap_or_else(|_| unreachable!());
    let chip = tr_args(
        "controls-binding-chip",
        &[(
            "binding",
            modplayer_ui::key_label::KeyLabel::new(chord, platform_now()).spoken,
        )],
    );
    assert!(!bounds_labelled(&update, &chip).is_empty(), "chip rendered");
    assert!(
        bounds_labelled(&update, &tr("controls-add-binding")).is_empty(),
        "`Add binding` must be absent at rest"
    );
    assert!(
        update.nodes.iter().all(|(_, n)| n.role() != Role::Button
            || !n.label().is_some_and(|l| (l.starts_with("Reset ")
                && l != tr("controls-reset-all"))
                || l.starts_with("Remove binding"))),
        "no Remove/Reset buttons at rest"
    );
}

#[test]
fn unbound_row_shows_not_bound() {
    let (mut controller, _dir) = fresh_controller("unbound");
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut screen = ControlsScreen::default();
    let action = HostAction::ToggleLoop;
    let chord = Chord::parse("L").unwrap_or_else(|_| unreachable!());
    controller.remove_binding(ActionId::Host(action), chord);
    let texts = rendered_texts(&ctx, &mut controller, &mut screen);
    assert!(
        texts.iter().any(|t| t == &tr("shortcuts-not-bound")),
        "an unbound row must say `Not bound`"
    );
}

#[test]
fn trigger_kind_label_removed() {
    let (mut controller, _dir) = fresh_controller("kind-label");
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut screen = ControlsScreen::default();
    let texts = rendered_texts(&ctx, &mut controller, &mut screen);
    assert!(
        !texts.iter().any(|t| t == &tr("controls-kind-trigger")),
        "the `trigger` kind label must not be drawn"
    );
    let groups = modplayer_ui::shortcut_groups::snapshot(controller.actions());
    let any_continuous = groups
        .iter()
        .flat_map(|g| &g.rows)
        .any(|r| r.kind == modplayer_core::actions::ActionKind::Continuous);
    assert_eq!(
        texts.iter().any(|t| t == &tr("controls-kind-continuous")),
        any_continuous,
        "a kind label is drawn for continuous actions only"
    );
}

#[test]
fn chip_text_is_key_label() {
    let (mut controller, _dir) = fresh_controller("chip-text");
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut screen = ControlsScreen::default();
    let _ = render_update(&ctx, &mut controller, &mut screen);
    let update = render_update(&ctx, &mut controller, &mut screen);
    let chord = Chord::parse("Space").unwrap_or_else(|_| unreachable!());
    let label = modplayer_ui::key_label::KeyLabel::new(chord, platform_now());
    assert_eq!(label.visible, "Space");
    let chip = tr_args("controls-binding-chip", &[("binding", label.spoken)]);
    let node = update
        .nodes
        .iter()
        .find(|(_, n)| n.label() == Some(chip.as_str()))
        .map(|(_, n)| n)
        .expect("Space chip");
    assert_eq!(
        node.value(),
        Some("Space"),
        "the chip paints the key label only, with no `Binding` prefix"
    );
}

fn controls_hidden_at_rest_take_zero_width_body() {
    let (mut controller, _dir) = fresh_controller("zero-width");
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut screen = ControlsScreen::default();
    let action = HostAction::ToggleLoop;
    let add = tr("controls-add-binding");
    let reset = tr_args(
        "controls-reset-action",
        &[("action", tr(action.label_key()))],
    );
    let chord = Chord::parse("L").unwrap_or_else(|_| unreachable!());
    let chip = tr_args(
        "controls-binding-chip",
        &[(
            "binding",
            modplayer_ui::key_label::KeyLabel::new(chord, platform_now()).spoken,
        )],
    );

    let _ = render_update(&ctx, &mut controller, &mut screen);
    let rest = render_update(&ctx, &mut controller, &mut screen);
    assert!(bounds_labelled(&rest, &add).is_empty());
    assert!(bounds_labelled(&rest, &reset).is_empty());
    let rest_chip = bounds_labelled(&rest, &chip);
    assert_eq!(rest_chip.len(), 1);

    hover_row(&ctx, &mut controller, &mut screen, action);
    assert!(
        has_button(&ctx, &mut controller, &mut screen, &reset),
        "hover reveals the row's controls"
    );

    // Pointer leaves: back to rest — nodes absent, row layout as before.
    pointer_to(&ctx, &mut controller, &mut screen, Pos2::new(1.0, 1.0));
    let again = render_update(&ctx, &mut controller, &mut screen);
    assert!(bounds_labelled(&again, &add).is_empty());
    assert!(bounds_labelled(&again, &reset).is_empty());
    assert_eq!(bounds_labelled(&again, &chip), rest_chip);
}

#[test]
fn controls_hidden_at_rest_take_zero_width() {
    controls_hidden_at_rest_take_zero_width_body();
}

// -- C10-C13: reveal --------------------------------------------------------

#[test]
fn hover_reveals_controls() {
    let (mut controller, _dir) = fresh_controller("hover-reveal");
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut screen = ControlsScreen::default();
    let action = HostAction::ToggleLoop;
    let chord = Chord::parse("L").unwrap_or_else(|_| unreachable!());
    let remove = tr_args(
        "controls-remove-binding",
        &[(
            "binding",
            modplayer_ui::key_label::KeyLabel::new(chord, platform_now()).spoken,
        )],
    );
    let add = tr("controls-add-binding");
    let reset = tr_args(
        "controls-reset-action",
        &[("action", tr(action.label_key()))],
    );
    hover_row(&ctx, &mut controller, &mut screen, action);
    let update = render_update(&ctx, &mut controller, &mut screen);
    assert!(!bounds_labelled(&update, &remove).is_empty(), "× revealed");
    assert!(!bounds_labelled(&update, &add).is_empty(), "Add revealed");
    assert!(
        !bounds_labelled(&update, &reset).is_empty(),
        "Reset revealed"
    );
    // Only the hovered row is revealed.
    assert_eq!(bounds_labelled(&update, &add).len(), 1);
}

fn focus_node<B: OutputBackend, H: SourceHost>(
    ctx: &Context,
    controller: &mut PlaybackController<B, H>,
    screen: &mut ControlsScreen,
    target: egui::accesskit::NodeId,
) {
    let mut input = default_input();
    input.events.push(Event::AccessKitActionRequest(
        egui::accesskit::ActionRequest {
            action: egui::accesskit::Action::Focus,
            target_tree: egui::accesskit::TreeId::ROOT,
            target_node: target,
            data: None,
        },
    ));
    let output = ctx.run_ui(input, |ui| controls::show(ui, controller, screen, None));
    output.drop_without_applying_deltas();
}

#[test]
fn row_element_is_focusable_and_named() {
    let (mut controller, _dir) = fresh_controller("row-element");
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut screen = ControlsScreen::default();
    let action = HostAction::ToggleLoop;
    let name = tr_args(
        "controls-row",
        &[
            ("action", tr(action.label_key())),
            ("bindings", spoken_bindings(&controller, action)),
        ],
    );
    let _ = render_update(&ctx, &mut controller, &mut screen);
    let update = render_update(&ctx, &mut controller, &mut screen);
    let node = update
        .nodes
        .iter()
        .find(|(_, n)| n.label() == Some(name.as_str()))
        .map(|(_, n)| n)
        .unwrap_or_else(|| panic!("row element `{name}`"));
    assert!(
        node.supports_action(egui::accesskit::Action::Focus),
        "the row element must be focusable"
    );
}

#[test]
fn keyboard_reveal_tab_walk() {
    let (mut controller, _dir) = fresh_controller("tab-walk");
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut screen = ControlsScreen::default();
    let action = HostAction::ToggleLoop;
    let chord = Chord::parse("L").unwrap_or_else(|_| unreachable!());
    let name = tr_args(
        "controls-row",
        &[
            ("action", tr(action.label_key())),
            ("bindings", spoken_bindings(&controller, action)),
        ],
    );
    let remove = tr_args(
        "controls-remove-binding",
        &[(
            "binding",
            modplayer_ui::key_label::KeyLabel::new(chord, platform_now()).spoken,
        )],
    );
    let add = tr("controls-add-binding");
    let reset = tr_args(
        "controls-reset-action",
        &[("action", tr(action.label_key()))],
    );

    let _ = render_update(&ctx, &mut controller, &mut screen);
    let update = render_update(&ctx, &mut controller, &mut screen);
    let row_node = update
        .nodes
        .iter()
        .find(|(_, n)| n.label() == Some(name.as_str()))
        .map(|(id, _)| *id)
        .expect("row element");
    focus_node(&ctx, &mut controller, &mut screen, row_node);
    let _ = render_update(&ctx, &mut controller, &mut screen);
    let update = render_update(&ctx, &mut controller, &mut screen);
    assert_eq!(update.focus, row_node, "row element holds focus");
    assert!(
        !bounds_labelled(&update, &remove).is_empty(),
        "focus reveals"
    );

    let node_named = |u: &TreeUpdate, label: &str| {
        u.nodes
            .iter()
            .find(|(_, n)| n.role() == Role::Button && n.label() == Some(label))
            .map(|(id, _)| *id)
            .unwrap_or_else(|| panic!("button `{label}`"))
    };
    for expected in [&remove, &add, &reset] {
        let mut input = default_input();
        input.events.push(key(EguiKey::Tab, Modifiers::NONE));
        let mut output = ctx.run_ui(input, |ui| {
            controls::show(ui, &mut controller, &mut screen, None)
        });
        let update = output
            .platform_output
            .accesskit_update
            .take()
            .expect("accesskit");
        output.drop_without_applying_deltas();
        assert_eq!(
            update.focus,
            node_named(&update, expected),
            "Tab order: row element, ×, Add binding, Reset; expected `{expected}`"
        );
    }

    // Focus moves elsewhere (the filter box): controls hide again.
    let filter = update
        .nodes
        .iter()
        .find(|(_, n)| n.role() == Role::TextInput)
        .map(|(id, _)| *id)
        .expect("filter");
    focus_node(&ctx, &mut controller, &mut screen, filter);
    let _ = render_update(&ctx, &mut controller, &mut screen);
    assert!(!has_button(&ctx, &mut controller, &mut screen, &reset));
}

#[test]
fn capture_keeps_controls_visible_when_pointer_leaves() {
    let (mut controller, _dir) = fresh_controller("capture-visible");
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut screen = ControlsScreen::default();
    let action = HostAction::ToggleLoop;
    let reset = tr_args(
        "controls-reset-action",
        &[("action", tr(action.label_key()))],
    );
    hover_row(&ctx, &mut controller, &mut screen, action);
    let bounds = nth_button_bounds(
        &ctx,
        &mut controller,
        &mut screen,
        &tr("controls-add-binding"),
        0,
    );
    click_at(&ctx, &mut controller, &mut screen, bounds.center());
    assert_eq!(screen.capture, Some(ActionId::Host(action)));

    pointer_to(&ctx, &mut controller, &mut screen, Pos2::new(1.0, 1.0));
    assert_eq!(screen.capture, Some(ActionId::Host(action)));
    assert!(
        has_button(&ctx, &mut controller, &mut screen, &reset),
        "controls stay visible while capturing"
    );

    // A rejection (Tab) keeps them visible too.
    press_key(
        &ctx,
        &mut controller,
        &mut screen,
        key(EguiKey::Tab, Modifiers::NONE),
    );
    assert!(screen.capture_error.is_some());
    pointer_to(&ctx, &mut controller, &mut screen, Pos2::new(2.0, 2.0));
    assert!(has_button(&ctx, &mut controller, &mut screen, &reset));
}

// -- C17: size --------------------------------------------------------------

#[test]
fn controls_height_at_most_60_percent_of_baseline() {
    let height = measure_controls_content_height();
    assert!(
        height <= 0.6 * PRE_029_CONTROLS_HEIGHT,
        "controls page is {height} px; limit is {} px (60% of {PRE_029_CONTROLS_HEIGHT})",
        0.6 * PRE_029_CONTROLS_HEIGHT
    );
}

// -- NFR-7.1: 40% pseudo-expansion ------------------------------------------

#[test]
fn many_bindings_wrap_without_elision_pseudo_expanded() {
    modplayer_core::i18n::with_pseudo_expansion(40, many_bindings_wrap_without_elision_body);
}

#[test]
fn controls_hidden_at_rest_take_zero_width_pseudo_expanded() {
    modplayer_core::i18n::with_pseudo_expansion(40, controls_hidden_at_rest_take_zero_width_body);
}

// ---------------------------------------------------------------------
// 029-keyboard-shortcut-reference, US2 (contracts C14-C16)
// ---------------------------------------------------------------------

/// Action ids (host) that survive `query` on `platform` in a default
/// registry, via the filter the controls page uses.
fn filtered_actions(
    controller: &PlaybackController<FakeBackend, SyntheticHost>,
    query: &str,
    platform: Platform,
) -> Vec<ActionId> {
    let groups = modplayer_ui::shortcut_groups::snapshot(controller.actions());
    modplayer_ui::shortcut_groups::filter_groups(&groups, query, platform)
        .into_iter()
        .flat_map(|(_, rows)| rows.into_iter().map(|r| r.row.id.clone()))
        .collect()
}

#[test]
fn filter_by_key_alias_lists_bound_actions() {
    let (mut controller, _dir) = fresh_controller("filter-key-alias");
    let loop_id = ActionId::Host(HostAction::ToggleLoop);
    let play_id = ActionId::Host(HostAction::TogglePlayPause);
    let ctrl_shift_l = Chord::parse("Primary+Shift+L").unwrap_or_else(|_| unreachable!());
    controller
        .add_binding(HostAction::ToggleLoop, ctrl_shift_l)
        .unwrap_or_else(|_| unreachable!());

    for query in ["L", "l", " l "] {
        assert!(
            filtered_actions(&controller, query, Platform::Other).contains(&loop_id),
            "`{query}` must list Toggle loop"
        );
    }
    for query in ["space", "Space"] {
        assert!(
            filtered_actions(&controller, query, Platform::Other).contains(&play_id),
            "`{query}` must list Play/pause"
        );
    }
    // Canonical key name (what the user types for an arrow key).
    let seek_left: Vec<ActionId> = HostAction::ALL
        .into_iter()
        .filter(|a| {
            controller
                .actions()
                .bindings(*a)
                .iter()
                .any(|c| c.key.as_str() == "Left" && c.mods.is_none())
        })
        .map(ActionId::Host)
        .collect();
    assert!(
        !seek_left.is_empty(),
        "fixture: some action binds bare Left"
    );
    let by_name = filtered_actions(&controller, "left", Platform::Other);
    for id in &seek_left {
        assert!(by_name.contains(id), "`left` must list {id:?}");
    }
    // Mac arrow glyph.
    let by_glyph = filtered_actions(&controller, "\u{2190}", Platform::Mac);
    for id in &seek_left {
        assert!(by_glyph.contains(id), "`←` (mac) must list {id:?}");
    }
    // Whole visible chord.
    assert!(
        filtered_actions(&controller, "ctrl+shift+l", Platform::Other).contains(&loop_id),
        "`ctrl+shift+l` must list the chord's action"
    );
}

#[test]
fn filter_key_match_is_exact() {
    let (controller, _dir) = fresh_controller("filter-key-exact");
    let groups = modplayer_ui::shortcut_groups::snapshot(controller.actions());
    let hits = modplayer_ui::shortcut_groups::filter_groups(&groups, "e", Platform::Other);
    for (_, rows) in &hits {
        for filtered in rows {
            for (chord, &hit) in filtered.row.bindings.iter().zip(&filtered.key_hits) {
                if hit {
                    assert_eq!(
                        chord.key.as_str(),
                        "E",
                        "`e` may key-match only the E key, got `{}`",
                        chord.key.as_str()
                    );
                }
            }
        }
    }
    // Enter / Escape actions are not listed through their keys.
    let via_keys: Vec<&str> = hits
        .iter()
        .flat_map(|(_, rows)| rows.iter())
        .filter(|f| f.key_hits.iter().any(|&h| h))
        .flat_map(|f| f.row.bindings.iter().map(|c| c.key.as_str()))
        .collect();
    assert!(!via_keys.contains(&"Enter") && !via_keys.contains(&"Escape"));
}

#[test]
fn filter_no_match_message() {
    let (mut controller, _dir) = fresh_controller("filter-key-nomatch");
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut screen = ControlsScreen {
        filter: "ctrl+alt+shift+zzz".to_string(),
        ..Default::default()
    };
    let texts = rendered_texts(&ctx, &mut controller, &mut screen);
    assert!(texts.iter().any(|t| t == &tr("controls-no-match")));
}

/// Number of rectangle strokes in `ctx`'s frame output equal to the
/// accent focus-ring stroke, after rendering the controls page once.
fn accent_outlined_rects(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, SyntheticHost>,
    screen: &mut ControlsScreen,
) -> usize {
    fn count(shape: &egui::Shape, ring: egui::Stroke) -> usize {
        match shape {
            egui::Shape::Vec(shapes) => shapes.iter().map(|s| count(s, ring)).sum(),
            egui::Shape::Rect(rect) if rect.stroke == ring => 1,
            _ => 0,
        }
    }
    let output = ctx.run_ui(default_input(), |ui| {
        controls::show(ui, controller, screen, None);
    });
    let roles = modplayer_ui::theme::roles(&ctx.global_style().visuals);
    let ring = modplayer_ui::theme::controls::focus_ring(roles);
    let n = output.shapes.iter().map(|s| count(&s.shape, ring)).sum();
    output.drop_without_applying_deltas();
    n
}

#[test]
fn key_match_chip_outlined() {
    let (mut controller, _dir) = fresh_controller("key-match-outline");
    let ctx = fresh_ctx();
    let mut plain = ControlsScreen::default();
    let _ = accent_outlined_rects(&ctx, &mut controller, &mut plain);
    let baseline = accent_outlined_rects(&ctx, &mut controller, &mut plain);

    let mut screen = ControlsScreen {
        filter: "space".to_string(),
        ..Default::default()
    };
    let _ = accent_outlined_rects(&ctx, &mut controller, &mut screen);
    let filtered = accent_outlined_rects(&ctx, &mut controller, &mut screen);
    assert!(
        filtered > baseline,
        "a key-matched chip must gain an accent outline ({filtered} vs {baseline})"
    );
}

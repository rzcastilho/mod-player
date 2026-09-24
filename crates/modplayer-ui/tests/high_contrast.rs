// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::disallowed_methods,
    // H3/H4 pin `LIGHT`/`DARK`/`*_HIGH_CONTRAST`'s `high_contrast` field —
    // a `pub const` — as a literal regression net; clippy's constant-folding
    // sees the assertion as compile-time-true and flags it, but the point
    // is exactly that it stays true across changes to `theme/tokens.rs`.
    clippy::assertions_on_constants
)]

//! 017-high-contrast-appearance: the value suite for the second,
//! independent appearance axis (contracts/high-contrast-tokens.md
//! H1-H4, H6, H11-H16, contracts/marker-outline.md S1-S5). The contrast floors themselves
//! (H17/H18) live in `design_token_contrast.rs`'s own extended block, kept
//! separate so the five pre-existing loops there stay verbatim (research
//! R14). M1-M3/M11 (the outline value) live in `markers.rs`/
//! `plugin_overlays.rs`, beside the suites they extend.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use egui::accesskit::{NodeId, Role, Toggled};
use egui::{Context, Event, Pos2, RawInput, Rect, Stroke, Theme as EguiTheme};
use modplayer_audio_io::FakeBackend;
use modplayer_audio_source_synthetic::ScriptedHost;
use modplayer_core::settings::SettingsStore;
use modplayer_core::settings_registry::search;
use modplayer_core::{PlaybackController, tr};
use modplayer_engine::Theme;
use modplayer_ui::settings::appearance;
use modplayer_ui::theme::controls::focus_ring;
use modplayer_ui::theme::style::build_style;
use modplayer_ui::theme::tokens::{
    self, DARK, DARK_HIGH_CONTRAST, LIGHT, LIGHT_HIGH_CONTRAST, divider_color_for, for_theme,
    is_high_contrast,
};
use modplayer_ui::theme::{apply_tokens, apply_tokens_for};

// ---------------------------------------------------------------------
// §1 — The table set (H1-H4)
// ---------------------------------------------------------------------

/// H1: `for_theme(dark, hc)` returns each of the four tables for its own
/// `(dark, hc)` pair and nothing else.
#[test]
fn for_theme_selects_all_four_tables() {
    assert_eq!(*for_theme(false, false), LIGHT);
    assert_eq!(*for_theme(false, true), LIGHT_HIGH_CONTRAST);
    assert_eq!(*for_theme(true, false), DARK);
    assert_eq!(*for_theme(true, true), DARK_HIGH_CONTRAST);
}

/// H2: the pre-existing selector keeps meaning "normal mode".
#[test]
fn for_dark_mode_is_the_normal_mode_selector() {
    for dark in [false, true] {
        assert_eq!(*tokens::for_dark_mode(dark), *for_theme(dark, false));
    }
}

/// H3: `LIGHT`/`DARK` hold their present literal values field by field —
/// the regression net for FR-002/US1 AS-3. Fails if an implementation
/// recolours in place instead of adding two new tables.
#[test]
fn normal_tables_are_unchanged() {
    assert_eq!(
        LIGHT.text_primary,
        egui::Color32::from_rgb(0x1c, 0x1c, 0x1e)
    );
    assert_eq!(
        LIGHT.text_secondary,
        egui::Color32::from_rgb(0x5b, 0x5b, 0x60)
    );
    assert_eq!(
        LIGHT.text_disabled,
        egui::Color32::from_rgb(0x82, 0x82, 0x83)
    );
    assert_eq!(
        LIGHT.surface_base,
        egui::Color32::from_rgb(0xff, 0xff, 0xff)
    );
    assert_eq!(
        LIGHT.surface_raised,
        egui::Color32::from_rgb(0xe8, 0xe8, 0xea)
    );
    assert_eq!(LIGHT.accent, egui::Color32::from_rgb(0x0a, 0x63, 0xc9));
    assert_eq!(
        LIGHT.text_on_accent,
        egui::Color32::from_rgb(0xff, 0xff, 0xff)
    );
    assert_eq!(LIGHT.positive, egui::Color32::from_rgb(0x1f, 0x7a, 0x44));
    assert_eq!(LIGHT.warning, egui::Color32::from_rgb(0x8a, 0x5a, 0x00));
    assert_eq!(LIGHT.danger, egui::Color32::from_rgb(0xb3, 0x26, 0x1e));
    assert!((LIGHT.disabled_alpha - 0.55).abs() < f32::EPSILON);
    assert!((LIGHT.divider_alpha - 0.08).abs() < f32::EPSILON);
    assert!(!LIGHT.high_contrast);

    assert_eq!(DARK.text_primary, egui::Color32::from_rgb(0xf2, 0xf2, 0xf7));
    assert_eq!(
        DARK.text_secondary,
        egui::Color32::from_rgb(0xa8, 0xa8, 0xb0)
    );
    assert_eq!(
        DARK.text_disabled,
        egui::Color32::from_rgb(0x76, 0x76, 0x7a)
    );
    assert_eq!(DARK.surface_base, egui::Color32::from_rgb(0x14, 0x14, 0x17));
    assert_eq!(
        DARK.surface_raised,
        egui::Color32::from_rgb(0x26, 0x26, 0x2c)
    );
    assert_eq!(DARK.accent, egui::Color32::from_rgb(0x5a, 0xa9, 0xff));
    assert_eq!(
        DARK.text_on_accent,
        egui::Color32::from_rgb(0x14, 0x14, 0x17)
    );
    assert_eq!(DARK.positive, egui::Color32::from_rgb(0x4c, 0xaf, 0x50));
    assert_eq!(DARK.warning, egui::Color32::from_rgb(0xe0, 0xa9, 0x2a));
    assert_eq!(DARK.danger, egui::Color32::from_rgb(0xff, 0x6b, 0x5e));
    assert!((DARK.disabled_alpha - 0.44).abs() < f32::EPSILON);
    assert!((DARK.divider_alpha - 0.08).abs() < f32::EPSILON);
    assert!(!DARK.high_contrast);
}

/// H4: `high_contrast` is `false` on both normal tables, `true` on both
/// high-contrast tables.
#[test]
fn the_axis_field_marks_exactly_the_two_high_contrast_tables() {
    assert!(!LIGHT.high_contrast);
    assert!(!DARK.high_contrast);
    assert!(LIGHT_HIGH_CONTRAST.high_contrast);
    assert!(DARK_HIGH_CONTRAST.high_contrast);
}

// ---------------------------------------------------------------------
// §2 — What high contrast does not change (H6, FR-002/FR-006/FR-010)
// ---------------------------------------------------------------------

/// H6: `text_primary`, `text_disabled`, `disabled_alpha` and
/// `text_on_accent` are untouched by the axis — only `text_secondary`
/// (H11) and the palette roles/divider/ring move. `text_on_accent`:
/// research R5 — FR-010's conditional does not fire for either table.
#[test]
fn unaffected_roles_are_identical_across_the_axis() {
    assert_eq!(LIGHT_HIGH_CONTRAST.text_primary, LIGHT.text_primary);
    assert_eq!(LIGHT_HIGH_CONTRAST.text_disabled, LIGHT.text_disabled);
    assert_eq!(LIGHT_HIGH_CONTRAST.text_on_accent, LIGHT.text_on_accent);
    assert!((LIGHT_HIGH_CONTRAST.disabled_alpha - LIGHT.disabled_alpha).abs() < f32::EPSILON);

    assert_eq!(DARK_HIGH_CONTRAST.text_primary, DARK.text_primary);
    assert_eq!(DARK_HIGH_CONTRAST.text_disabled, DARK.text_disabled);
    assert_eq!(DARK_HIGH_CONTRAST.text_on_accent, DARK.text_on_accent);
    assert!((DARK_HIGH_CONTRAST.disabled_alpha - DARK.disabled_alpha).abs() < f32::EPSILON);
}

// ---------------------------------------------------------------------
// §3 — The promoted secondary role (H11-H13, FR-005)
// ---------------------------------------------------------------------

/// H11.
#[test]
fn high_contrast_promotes_secondary_text() {
    assert_eq!(
        LIGHT_HIGH_CONTRAST.text_secondary,
        LIGHT_HIGH_CONTRAST.text_primary
    );
    assert_eq!(
        DARK_HIGH_CONTRAST.text_secondary,
        DARK_HIGH_CONTRAST.text_primary
    );
}

/// H12: the precondition `is_high_contrast` reads (research R2) — without
/// it the predicate silently misclassifies.
#[test]
fn normal_tables_keep_secondary_distinct() {
    assert_ne!(LIGHT.text_secondary, LIGHT.text_primary);
    assert_ne!(DARK.text_secondary, DARK.text_primary);
}

/// H13: `weak_text_color`/`weak_text_alpha` follow the promotion in a
/// built high-contrast style.
#[test]
fn weak_text_follows_the_promotion() {
    for (theme, roles) in [
        (EguiTheme::Light, &LIGHT_HIGH_CONTRAST),
        (EguiTheme::Dark, &DARK_HIGH_CONTRAST),
    ] {
        let style = build_style(theme, true);
        assert_eq!(style.visuals.weak_text_color, Some(roles.text_primary));
        assert_eq!(style.visuals.weak_text_alpha, 1.0);
    }
}

// ---------------------------------------------------------------------
// §4 — The divider (H14-H15, FR-007)
// ---------------------------------------------------------------------

/// H14.
#[test]
fn divider_is_the_primary_text_colour_scaled() {
    for roles in [&LIGHT, &DARK, &LIGHT_HIGH_CONTRAST, &DARK_HIGH_CONTRAST] {
        assert_eq!(
            divider_color_for(roles),
            roles.text_primary.gamma_multiply(roles.divider_alpha)
        );
    }
    assert!((LIGHT.divider_alpha - 0.08).abs() < f32::EPSILON);
    assert!((DARK.divider_alpha - 0.08).abs() < f32::EPSILON);
    assert!((LIGHT_HIGH_CONTRAST.divider_alpha - 1.0).abs() < f32::EPSILON);
    assert!((DARK_HIGH_CONTRAST.divider_alpha - 1.0).abs() < f32::EPSILON);
}

/// H15: in a high-contrast style, every border follows the divider.
#[test]
fn every_border_follows_the_divider() {
    for (theme, roles) in [
        (EguiTheme::Light, &LIGHT_HIGH_CONTRAST),
        (EguiTheme::Dark, &DARK_HIGH_CONTRAST),
    ] {
        let style = build_style(theme, true);
        let divider = divider_color_for(roles);
        let expected = Stroke::new(1.0, divider);
        let widgets = &style.visuals.widgets;
        for widget in [
            &widgets.noninteractive,
            &widgets.inactive,
            &widgets.hovered,
            &widgets.active,
            &widgets.open,
        ] {
            assert_eq!(widget.bg_stroke, expected);
        }
        assert_eq!(style.visuals.window_stroke, expected);
    }
}

// ---------------------------------------------------------------------
// §5 — The focus ring (H16, FR-008)
// ---------------------------------------------------------------------

/// H16.
#[test]
fn focus_ring_thickens_only_in_high_contrast() {
    for roles in [&LIGHT, &DARK] {
        let ring = focus_ring(roles);
        assert_eq!(ring.width, 2.0);
        assert_eq!(ring.color, roles.accent);
    }
    for roles in [&LIGHT_HIGH_CONTRAST, &DARK_HIGH_CONTRAST] {
        let ring = focus_ring(roles);
        assert_eq!(ring.width, 3.0);
        assert_eq!(ring.color, roles.accent);
    }
}

// ---------------------------------------------------------------------
// §4 (marker-outline.md) — One selection site (S1, S2, S4, S5, FR-017)
// ---------------------------------------------------------------------

/// S1: `is_high_contrast` is recoverable from the applied `Visuals` alone,
/// including the "bare visuals" case existing unit tests rely on.
#[test]
fn high_contrast_is_recoverable_from_the_applied_visuals() {
    for theme in [EguiTheme::Light, EguiTheme::Dark] {
        assert!(is_high_contrast(&build_style(theme, true).visuals));
        assert!(!is_high_contrast(&build_style(theme, false).visuals));
    }
    assert!(!is_high_contrast(&egui::Visuals::light()));
    assert!(!is_high_contrast(&egui::Visuals::dark()));
}

/// S2: `roles(v)` returns the table matching `(v.dark_mode,
/// is_high_contrast(v))` for all four built styles.
#[test]
fn roles_selects_the_table_the_style_was_built_from() {
    for (theme, dark, hc) in [
        (EguiTheme::Light, false, false),
        (EguiTheme::Dark, true, false),
        (EguiTheme::Light, false, true),
        (EguiTheme::Dark, true, true),
    ] {
        let visuals = build_style(theme, hc).visuals;
        assert_eq!(*tokens::roles(&visuals), *for_theme(dark, hc));
    }
}

/// S4: `apply_tokens(ctx)` still installs the normal-mode pair, and
/// `apply_tokens_for(ctx, false)` installs the identical `Arc`s.
#[test]
fn apply_tokens_defaults_to_normal_mode() {
    egui::__run_test_ctx(|ctx| {
        apply_tokens(ctx);
        let light = ctx.style_of(EguiTheme::Light);
        let dark = ctx.style_of(EguiTheme::Dark);
        assert!(!light.visuals.dark_mode);
        assert!(dark.visuals.dark_mode);
        assert!(!modplayer_ui::theme::tokens::is_high_contrast(
            &light.visuals
        ));

        apply_tokens_for(ctx, false);
        assert!(std::sync::Arc::ptr_eq(
            &light,
            &ctx.style_of(EguiTheme::Light)
        ));
        assert!(std::sync::Arc::ptr_eq(
            &dark,
            &ctx.style_of(EguiTheme::Dark)
        ));
    });
}

/// S5: `apply_tokens_for` allocates nothing after the first call — a
/// repeated call at either setting returns `Arc::ptr_eq` styles.
#[test]
fn apply_tokens_for_is_allocation_free_after_the_first_call() {
    egui::__run_test_ctx(|ctx| {
        apply_tokens_for(ctx, true);
        let first_light = ctx.style_of(EguiTheme::Light);
        let first_dark = ctx.style_of(EguiTheme::Dark);

        apply_tokens_for(ctx, true);
        assert!(std::sync::Arc::ptr_eq(
            &first_light,
            &ctx.style_of(EguiTheme::Light)
        ));
        assert!(std::sync::Arc::ptr_eq(
            &first_dark,
            &ctx.style_of(EguiTheme::Dark)
        ));

        apply_tokens_for(ctx, false);
        let second_light = ctx.style_of(EguiTheme::Light);
        apply_tokens_for(ctx, false);
        assert!(std::sync::Arc::ptr_eq(
            &second_light,
            &ctx.style_of(EguiTheme::Light)
        ));
    });
}

// ---------------------------------------------------------------------
// §6 — The Appearance screen's checkbox (contracts/appearance-setting.md
// A14, A16-A20; User Story 2, Phase 4)
// ---------------------------------------------------------------------

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-ui-high-contrast-{label}-{}-{unique}",
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

/// `PlaybackController::new` makes one synchronous read of
/// `MODPLAYER_TRACK_STATE_DIR`; serialize every construction in this
/// binary against it and scope the override narrowly (mirrors
/// `accessibility.rs`'s identically-named lock).
static TRACK_STATE_ENV_LOCK: Mutex<()> = Mutex::new(());

fn with_track_state_dir<T>(dir: &Path, f: impl FnOnce() -> T) -> T {
    let _guard = TRACK_STATE_ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // Safety: narrowly scopes the mutation to the one synchronous read
    // `PlaybackController::new` does of this var, serialized against every
    // other test in this binary via the lock above.
    unsafe { std::env::set_var("MODPLAYER_TRACK_STATE_DIR", dir) };
    let result = f();
    unsafe { std::env::remove_var("MODPLAYER_TRACK_STATE_DIR") };
    result
}

fn fresh_controller(
    label: &str,
) -> (
    PlaybackController<FakeBackend, ScriptedHost>,
    PathBuf,
    TempDir,
    TempDir,
) {
    let (store, dir) = fresh_store(label);
    let path = store.path().to_path_buf();
    let track_state_dir = TempDir::new(&format!("{label}-track-state"));
    let controller = with_track_state_dir(track_state_dir.path(), || {
        PlaybackController::new(FakeBackend::new(vec![]), ScriptedHost::new(), store)
    });
    (controller, path, dir, track_state_dir)
}

fn default_input() -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
        ..Default::default()
    }
}

fn fresh_appearance_ctx() -> Context {
    let ctx = Context::default();
    apply_tokens(&ctx);
    ctx.enable_accesskit();
    ctx
}

/// One AccessKit node's accessibility-relevant fields, plus its own id
/// (needed to check `update.focus`) and its raw bounds (the checkbox's own
/// rect, to drive a synthetic pointer event at) — mirrors
/// `settings_plugins.rs`'s identically-named type.
#[derive(Debug, Clone)]
struct AccessNode {
    id: NodeId,
    role: Role,
    label: Option<String>,
    value: Option<String>,
    toggled: Option<Toggled>,
    bounds: Option<Rect>,
}

impl AccessNode {
    fn accessible_name(&self) -> Option<&str> {
        self.label.as_deref().or(self.value.as_deref())
    }
}

fn run_frame(
    ctx: &Context,
    input: RawInput,
    mut render: impl FnMut(&mut egui::Ui),
) -> (Option<NodeId>, Vec<AccessNode>) {
    let mut output = ctx.run_ui(input, |ui| render(ui));
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    let focus = update.focus;
    output.drop_without_applying_deltas();

    let nodes = update
        .nodes
        .iter()
        .map(|(id, node)| AccessNode {
            id: *id,
            role: node.role(),
            label: node.label().map(str::to_string),
            value: node.value().map(str::to_string),
            toggled: node.toggled(),
            bounds: node.bounds().map(|b| {
                Rect::from_min_max(
                    Pos2::new(b.x0 as f32, b.y0 as f32),
                    Pos2::new(b.x1 as f32, b.y1 as f32),
                )
            }),
        })
        .collect();
    (Some(focus), nodes)
}

fn find_all<'a>(nodes: &'a [AccessNode], role: Role, name: &str) -> Vec<&'a AccessNode> {
    nodes
        .iter()
        .filter(|node| node.role == role && node.accessible_name() == Some(name))
        .collect()
}

fn find_one<'a>(nodes: &'a [AccessNode], role: Role, name: &str) -> &'a AccessNode {
    let matches = find_all(nodes, role, name);
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one {role:?} node named `{name}`, found {}: {nodes:?}",
        matches.len()
    );
    matches[0]
}

/// A14: the Appearance screen renders a checkbox below the Theme combo
/// whose accessible node has `Role::CheckBox`, a non-empty label from
/// `tr("setting-high-contrast")`, and a `Toggled` state matching the
/// setting — not a fourth entry inside the Theme combo.
#[test]
fn appearance_screen_shows_a_high_contrast_checkbox() {
    let (mut controller, _path, _dir, _track_state_dir) = fresh_controller("checkbox-node");
    let ctx = fresh_appearance_ctx();
    let mut cached = controller.settings_store().load().settings;

    let (_, nodes) = run_frame(&ctx, default_input(), |ui| {
        appearance::show(ui, &mut controller, &mut cached, None);
    });

    let checkbox = find_one(&nodes, Role::CheckBox, &tr("setting-high-contrast"));
    assert_eq!(
        checkbox.toggled,
        Some(Toggled::False),
        "off by default: {checkbox:?}"
    );
    assert!(
        nodes.iter().any(|n| n.role == Role::ComboBox),
        "the Theme combo must still render as its own ComboBox, not be replaced or \
         swallow the new control: {nodes:?}"
    );
}

/// A16: a Settings search for the setting's own title finds
/// `appearance.high_contrast`; arriving on the Appearance screen with
/// `focus == Some("appearance.high_contrast")` leaves keyboard focus on
/// the checkbox, not the Theme combo.
#[test]
fn high_contrast_is_searchable_and_focusable() {
    let hits = search(&tr("setting-high-contrast"));
    assert!(
        hits.iter().any(|d| d.id == "appearance.high_contrast"),
        "searching the setting's own title must surface appearance.high_contrast: {hits:?}"
    );

    let (mut controller, _path, _dir, _track_state_dir) = fresh_controller("checkbox-focus");
    let ctx = fresh_appearance_ctx();
    let mut cached = controller.settings_store().load().settings;

    // Frame 1: arrives with the search hit's focus request.
    let (_, nodes) = run_frame(&ctx, default_input(), |ui| {
        appearance::show(
            ui,
            &mut controller,
            &mut cached,
            Some("appearance.high_contrast"),
        );
    });
    let checkbox_id = find_one(&nodes, Role::CheckBox, &tr("setting-high-contrast")).id;

    // Frame 2: the requested focus is now reflected in `update.focus`
    // (mirrors `settings_plugins.rs`'s own `search_finds_field_with_path`).
    let (focused, _) = run_frame(&ctx, default_input(), |ui| {
        appearance::show(ui, &mut controller, &mut cached, None);
    });
    assert_eq!(
        focused,
        Some(checkbox_id),
        "focus must land on the checkbox, not the Theme combo"
    );
}

/// A17: toggling the checkbox calls `controller.set_high_contrast(on)` and
/// persists through the same reload-mutate-save block the Theme combo uses.
#[test]
fn toggling_the_checkbox_persists_and_updates_the_controller() {
    let (mut controller, path, _dir, _track_state_dir) = fresh_controller("toggle-persist");
    let ctx = fresh_appearance_ctx();
    let mut cached = controller.settings_store().load().settings;
    assert!(!controller.high_contrast());

    let (_, nodes) = run_frame(&ctx, default_input(), |ui| {
        appearance::show(ui, &mut controller, &mut cached, None);
    });
    let rect = find_one(&nodes, Role::CheckBox, &tr("setting-high-contrast"))
        .bounds
        .expect("the checkbox must have bounds");

    let mut press = default_input();
    press.events.push(Event::PointerButton {
        pos: rect.center(),
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    run_frame(&ctx, press, |ui| {
        appearance::show(ui, &mut controller, &mut cached, None);
    });
    let mut release = default_input();
    release.events.push(Event::PointerButton {
        pos: rect.center(),
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    run_frame(&ctx, release, |ui| {
        appearance::show(ui, &mut controller, &mut cached, None);
    });

    assert!(
        controller.high_contrast(),
        "clicking the checkbox must call set_high_contrast(true)"
    );
    assert!(
        cached.high_contrast,
        "the Settings screen's cached snapshot must be refreshed too"
    );
    assert!(
        SettingsStore::with_path(path).load().settings.high_contrast,
        "the toggle must persist through the reload-mutate-save block"
    );
}

/// A18: the frame after a toggle renders with the new tables — mirrors
/// `App::ui`'s own "apply first, then draw" ordering (`app.rs:223`), so no
/// intermediate frame ever renders with the old ones.
#[test]
fn the_next_frame_uses_the_new_tables() {
    let (mut controller, _path, _dir, _track_state_dir) = fresh_controller("next-frame-tables");
    let ctx = fresh_appearance_ctx();
    let mut cached = controller.settings_store().load().settings;

    // Frame 1, App::ui's own ordering: apply first, then draw — still
    // normal mode.
    apply_tokens_for(&ctx, controller.high_contrast());
    assert!(!is_high_contrast(&ctx.style_of(EguiTheme::Light).visuals));
    let (_, nodes) = run_frame(&ctx, default_input(), |ui| {
        appearance::show(ui, &mut controller, &mut cached, None);
    });
    let rect = find_one(&nodes, Role::CheckBox, &tr("setting-high-contrast"))
        .bounds
        .expect("the checkbox must have bounds");

    let mut press = default_input();
    press.events.push(Event::PointerButton {
        pos: rect.center(),
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    run_frame(&ctx, press, |ui| {
        appearance::show(ui, &mut controller, &mut cached, None);
    });
    let mut release = default_input();
    release.events.push(Event::PointerButton {
        pos: rect.center(),
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    run_frame(&ctx, release, |ui| {
        appearance::show(ui, &mut controller, &mut cached, None);
    });
    assert!(
        controller.high_contrast(),
        "the click must have toggled it on"
    );

    // Frame 2: App::ui applies first, before any widget runs — the
    // installed style is already the high-contrast one.
    apply_tokens_for(&ctx, controller.high_contrast());
    assert!(is_high_contrast(&ctx.style_of(EguiTheme::Light).visuals));
}

/// A19: after a simulated restart (a fresh controller over the same
/// settings file), both `theme` and `high_contrast` are exactly as left.
#[test]
fn both_axes_survive_a_restart() {
    let (controller, path, _dir, track_state_dir) = fresh_controller("restart");
    let mut settings = controller.settings_store().load().settings;
    settings.theme = Theme::Dark;
    settings.high_contrast = true;
    controller
        .settings_store()
        .save(&settings)
        .unwrap_or_else(|e| unreachable!("save: {e:?}"));
    drop(controller);

    let restarted = with_track_state_dir(track_state_dir.path(), || {
        PlaybackController::new(
            FakeBackend::new(vec![]),
            ScriptedHost::new(),
            SettingsStore::with_path(path),
        )
    });
    assert_eq!(restarted.theme(), Theme::Dark);
    assert!(restarted.high_contrast());
}

/// A20: toggling on and off repeatedly within consecutive frames leaves no
/// frame on a mixed palette — every frame's applied style is one of the
/// four whole tables, never a blend (Edge Case 2).
#[test]
fn rapid_toggling_never_produces_a_mixed_palette() {
    let (mut controller, _path, _dir, _track_state_dir) = fresh_controller("rapid-toggle");
    let ctx = fresh_appearance_ctx();

    for i in 0..20 {
        controller.set_high_contrast(i % 2 == 0);
        apply_tokens_for(&ctx, controller.high_contrast());
        let light = ctx.style_of(EguiTheme::Light);
        let dark = ctx.style_of(EguiTheme::Dark);
        assert_eq!(
            is_high_contrast(&light.visuals),
            controller.high_contrast(),
            "frame {i}: Light style must match the shadow state exactly, never a blend"
        );
        assert_eq!(
            is_high_contrast(&dark.visuals),
            controller.high_contrast(),
            "frame {i}: Dark style must match the shadow state exactly, never a blend"
        );
        assert_eq!(
            *tokens::roles(&light.visuals),
            *for_theme(false, controller.high_contrast()),
            "frame {i}: must be one of the four whole tables, never a partial mix"
        );
    }
}

// ---------------------------------------------------------------------
// §5 (marker-outline.md) — The one selection site, mechanically (S3,
// FR-017; source scan in the shape of `design_token_literals.rs`)
// ---------------------------------------------------------------------

/// S3: outside `crates/modplayer-ui/src/theme/**` and
/// `crates/modplayer-ui/src/{app.rs, settings/appearance.rs}`, no file
/// under this crate's `src/` names `high_contrast` — no paint site, no
/// view and no plugin-facing file branches on it. `tests/` is a
/// different root entirely and is never scanned here.
#[derive(Debug)]
struct ScanHit {
    path: PathBuf,
    line: usize,
    text: String,
}

impl std::fmt::Display for ScanHit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}: {} — high_contrast must only be named at its selection site \
             (theme/) or its control site (app.rs, settings/appearance.rs); route this \
             through theme::roles()/is_high_contrast() instead (S3)",
            self.path.display(),
            self.line,
            self.text.trim(),
        )
    }
}

/// The sanctioned exception: every file under `src/theme/`.
fn is_selection_site(rel: &Path) -> bool {
    rel.components()
        .next()
        .is_some_and(|c| c.as_os_str() == "theme")
}

/// The sanctioned control sites: `app.rs` and `settings/appearance.rs`.
fn is_control_site(rel: &Path) -> bool {
    rel == Path::new("app.rs") || rel == Path::new("settings").join("appearance.rs")
}

/// `use` statements and doc comments name no behaviour — mirrors
/// `design_token_literals.rs`'s `is_excluded_line`.
fn is_excluded_scan_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("use ") || trimmed.starts_with("///") || trimmed.starts_with("//!")
}

fn collect_ui_src_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_ui_src_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

#[test]
fn high_contrast_is_named_only_at_its_selection_and_control_sites() {
    let src_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    collect_ui_src_files(&src_root, &mut files);
    assert!(
        files.len() > 20,
        "expected > 20 .rs files under {}, found {} — scan root is broken",
        src_root.display(),
        files.len()
    );

    let mut hits = Vec::new();
    for path in files {
        let rel = path
            .strip_prefix(&src_root)
            .unwrap_or_else(|e| unreachable!("{} is under {src_root:?}: {e}", path.display()));
        if is_selection_site(rel) || is_control_site(rel) {
            continue;
        }
        let Ok(contents) = std::fs::read_to_string(&path) else {
            continue;
        };
        for (idx, line) in contents.lines().enumerate() {
            if is_excluded_scan_line(line) {
                continue;
            }
            if line.contains("high_contrast") {
                hits.push(ScanHit {
                    path: path.clone(),
                    line: idx + 1,
                    text: line.to_string(),
                });
            }
        }
    }

    if !hits.is_empty() {
        let report: Vec<String> = hits.iter().map(ScanHit::to_string).collect();
        panic!(
            "expected zero high_contrast mentions outside the selection/control sites, \
             found {}:\n{}",
            hits.len(),
            report.join("\n"),
        );
    }
}

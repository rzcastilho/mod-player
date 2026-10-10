// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! 028-settings-fields-and-account (contracts/settings-fields.md): the
//! grouped, legible Settings fields. US1 covers F1–F3 (cards and headings),
//! F4 (untouched categories keep their structure), F5/F6 (help indent and
//! measure), F7 (units inside the value text) and F9 (range captions).
//!
//! Each category screen is drawn headlessly at 960×640 with AccessKit on
//! and its nodes read back (mirrors `settings_plugins.rs`'s harness).

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use egui::accesskit::Role;
use egui::{Context, Pos2, RawInput, Rect};
use modplayer_audio_io::FakeBackend;
use modplayer_audio_source_synthetic::ScriptedHost;
use modplayer_core::settings::{NUDGE_STEP_MS_RANGE, SettingsStore};
use modplayer_core::{PlaybackController, tr};
use modplayer_engine::CeilingDb;
use modplayer_ui::settings::playback::PlaybackScreen;
use modplayer_ui::settings::{appearance, audio, language, playback};

type Controller = PlaybackController<FakeBackend, ScriptedHost>;

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn controller(label: &str) -> (Controller, TempDir) {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "modplayer-ui-settings-fields-{label}-{}-{unique}",
        std::process::id()
    ));
    let _ = std::fs::create_dir_all(&dir);
    let store = SettingsStore::with_path(dir.join("settings.toml"));
    let controller = PlaybackController::new(FakeBackend::new(vec![]), ScriptedHost::new(), store);
    (controller, TempDir(dir))
}

fn fresh_ctx() -> Context {
    let ctx = Context::default();
    modplayer_ui::theme::apply_tokens(&ctx);
    ctx.enable_accesskit();
    ctx
}

#[derive(Debug, Clone)]
struct Node {
    id: u64,
    role: Role,
    text: String,
    bounds: Option<Rect>,
}

impl Node {
    fn left(&self) -> f32 {
        self.bounds.expect("node has bounds").min.x
    }
}

fn input() -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(960.0, 640.0))),
        ..Default::default()
    }
}

/// One frame of `render`; every AccessKit node with its role, its label or
/// value text, and its bounds.
fn frame(ctx: &Context, render: impl FnOnce(&mut egui::Ui)) -> Vec<Node> {
    let mut render = Some(render);
    let mut output = ctx.run_ui(input(), |ui| {
        if let Some(render) = render.take() {
            render(ui);
        }
    });
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit enabled");
    output.drop_without_applying_deltas();
    update
        .nodes
        .iter()
        .map(|(id, node)| Node {
            id: id.0,
            role: node.role(),
            text: node
                .label()
                .or_else(|| node.value())
                .unwrap_or_default()
                .to_string(),
            bounds: node.bounds().map(|b| {
                Rect::from_min_max(
                    Pos2::new(b.x0 as f32, b.y0 as f32),
                    Pos2::new(b.x1 as f32, b.y1 as f32),
                )
            }),
        })
        .collect()
}

fn audio_nodes(label: &str) -> Vec<Node> {
    let (mut c, _dir) = controller(label);
    let mut cached = c.settings_store().load().settings;
    let ctx = fresh_ctx();
    // Two frames: the first sizes the layout, the second is the settled one.
    let _ = frame(&ctx, |ui| {
        let _ = audio::show(ui, &mut c, &mut cached, None);
    });
    frame(&ctx, |ui| {
        let _ = audio::show(ui, &mut c, &mut cached, None);
    })
}

fn playback_nodes(label: &str) -> Vec<Node> {
    let (mut c, _dir) = controller(label);
    let mut screen = PlaybackScreen::new(&c);
    let ctx = fresh_ctx();
    let _ = frame(&ctx, |ui| playback::show(ui, &mut c, &mut screen, None));
    frame(&ctx, |ui| playback::show(ui, &mut c, &mut screen, None))
}

fn appearance_nodes(label: &str) -> Vec<Node> {
    let (mut c, _dir) = controller(label);
    let mut cached = c.settings_store().load().settings;
    let ctx = fresh_ctx();
    let _ = frame(&ctx, |ui| {
        appearance::show(ui, &mut c, &mut cached, None);
    });
    frame(&ctx, |ui| appearance::show(ui, &mut c, &mut cached, None))
}

fn language_nodes() -> Vec<Node> {
    let ctx = fresh_ctx();
    let _ = frame(&ctx, |ui| language::show(ui, None));
    frame(&ctx, |ui| language::show(ui, None))
}

fn headings(nodes: &[Node]) -> Vec<&Node> {
    let mut found: Vec<&Node> = nodes.iter().filter(|n| n.role == Role::Heading).collect();
    found.sort_by(|a, b| a.bounds.unwrap().min.y.total_cmp(&b.bounds.unwrap().min.y));
    found
}

fn heading_names(nodes: &[Node]) -> Vec<String> {
    headings(nodes).iter().map(|n| n.text.clone()).collect()
}

fn find<'a>(nodes: &'a [Node], text: &str) -> &'a Node {
    let matches: Vec<&Node> = nodes.iter().filter(|n| n.text == text).collect();
    assert!(!matches.is_empty(), "no node named `{text}` in {nodes:#?}");
    matches[0]
}

fn any_text_contains(nodes: &[Node], needle: &str) -> bool {
    nodes.iter().any(|n| n.text.contains(needle))
}

fn y_of(nodes: &[Node], text: &str) -> f32 {
    find(nodes, text).bounds.unwrap().min.y
}

/// F1, F3: Audio is an "Output" card then a "Level protection" card, each
/// header a heading with the un-uppercased name, and the fields sit under
/// the right header in today's order.
#[test]
fn f1_audio_has_output_then_level_protection_cards() {
    let nodes = audio_nodes("f1");
    assert_eq!(
        heading_names(&nodes),
        vec![
            tr("settings-group-output"),
            tr("settings-group-level-protection")
        ]
    );

    let output = y_of(&nodes, &tr("settings-group-output"));
    let level = y_of(&nodes, &tr("settings-group-level-protection"));
    let device = y_of(&nodes, &tr("setting-output-device"));
    let preset = y_of(&nodes, &tr("setting-buffer-preset"));
    let test = y_of(&nodes, &tr("setting-test-output-device"));
    let ceiling = y_of(&nodes, &tr("setting-limiter-ceiling"));
    let cap = y_of(&nodes, &tr("setting-safe-volume-cap"));

    assert!(output < device && device < preset && preset < test);
    assert!(
        test < level,
        "Test output device belongs to the Output card"
    );
    assert!(level < ceiling && ceiling < cap);
}

/// F2, F3: Playback, Appearance and Language group their fields.
#[test]
fn f2_other_categories_are_grouped() {
    let nodes = playback_nodes("f2-playback");
    assert_eq!(
        heading_names(&nodes),
        vec![
            tr("settings-group-connect-device"),
            tr("settings-group-markers")
        ]
    );
    let markers = y_of(&nodes, &tr("settings-group-markers"));
    assert!(y_of(&nodes, &tr("setting-device-name")) < markers);
    assert!(y_of(&nodes, &tr("setting-nudge-step")) > markers);

    let nodes = appearance_nodes("f2-appearance");
    assert_eq!(heading_names(&nodes), vec![tr("settings-group-theme")]);

    let nodes = language_nodes();
    assert_eq!(heading_names(&nodes), vec![tr("settings-group-language")]);
}

/// F3: card headers keep their natural case (only the painted text is
/// uppercased).
#[test]
fn f3_headings_are_not_uppercased() {
    for nodes in [audio_nodes("f3-audio"), playback_nodes("f3-playback")] {
        for heading in headings(&nodes) {
            assert_ne!(heading.text, heading.text.to_uppercase());
        }
    }
}

/// F5, F6: help is indented at least `space::SM` beyond its label and never
/// wider than the body measure.
#[test]
fn f5_f6_help_is_indented_and_measured() {
    let nodes = audio_nodes("f6");
    let ctx = fresh_ctx();
    let _ = frame(&ctx, |_| {});
    let measure = modplayer_ui::theme::body_measure(&ctx);
    for (label, help) in [
        ("setting-limiter-ceiling", "setting-limiter-ceiling-desc"),
        ("setting-buffer-preset", "setting-buffer-preset-desc"),
        ("setting-safe-volume-cap", "setting-safe-volume-cap-desc"),
    ] {
        let label = find(&nodes, &tr(label));
        let help = find(&nodes, &tr(help));
        assert!(
            help.left() >= label.left() + modplayer_ui::theme::space::SM - 0.5,
            "help `{}` must be indented from its label",
            help.text
        );
        assert!(
            help.bounds.unwrap().width() <= measure + 0.5,
            "help `{}` wider than the body measure",
            help.text
        );
    }
}

/// F7: units live inside the control's own value text, never in a
/// separate label.
#[test]
fn f7_units_are_inside_the_value_text() {
    let nodes = audio_nodes("f7-audio");
    assert!(
        nodes.iter().any(|n| n.text == "-1.0 dBFS"),
        "ceiling shows `-1.0 dBFS`: {nodes:#?}"
    );
    assert!(
        nodes.iter().any(|n| n.text == "50%"),
        "cap shows `50%`: {nodes:#?}"
    );
    assert!(
        nodes
            .iter()
            .all(|n| !(n.role == Role::Label && n.text == "dBFS")),
        "no separate unit label"
    );
    // The buffer preset keeps its latency suffix.
    assert!(any_text_contains(&nodes, "(~"));

    let nodes = playback_nodes("f7-playback");
    assert!(
        nodes.iter().any(|n| n.text == "10 ms"),
        "nudge shows `10 ms`: {nodes:#?}"
    );
}

/// F9: captions come from the same constants the controls clamp with.
#[test]
fn f9_range_captions_match_the_clamp_constants() {
    let nodes = audio_nodes("f9-audio");
    let ceiling = modplayer_ui::settings::field::format_range(
        modplayer_ui::settings::field::Unit::Dbfs,
        f64::from(CeilingDb::MIN),
        f64::from(CeilingDb::MAX),
    );
    assert_eq!(ceiling, "-6.0 to -0.1 dBFS");
    find(&nodes, &ceiling);
    find(&nodes, "0 to 100%");

    let nodes = playback_nodes("f9-playback");
    let nudge = modplayer_ui::settings::field::format_range(
        modplayer_ui::settings::field::Unit::Ms,
        f64::from(*NUDGE_STEP_MS_RANGE.start()),
        f64::from(*NUDGE_STEP_MS_RANGE.end()),
    );
    assert_eq!(nudge, "1 to 1000 ms");
    find(&nodes, &nudge);
}

/// F4 guard: the categories this feature does not own keep their existing
/// structure — none of them grows a group heading from this feature.
#[test]
fn f4_untouched_categories_have_no_new_group_headings() {
    let (mut c, _dir) = controller("f4");
    let ctx = fresh_ctx();
    let nodes = frame(&ctx, |ui| {
        modplayer_ui::settings::developer::show(ui, &mut c)
    });
    let group_names = [
        tr("settings-group-output"),
        tr("settings-group-level-protection"),
        tr("settings-group-connect-device"),
        tr("settings-group-markers"),
        tr("settings-group-theme"),
        tr("settings-group-language"),
    ];
    assert!(
        nodes
            .iter()
            .all(|n| !(n.role == Role::Heading && group_names.contains(&n.text)))
    );
}

// ---------------------------------------------------------------------
// US2 — per-field Reset (F10–F15, F28)
// ---------------------------------------------------------------------

use modplayer_audio_io::FakeDevice;
use modplayer_core::AudioSettings;
use modplayer_engine::{
    BufferPreset, DeviceId, FrameCount, SafeVolume, SampleRate, Theme, VolumePercent,
};

fn reset_name(label_key: &str) -> String {
    modplayer_core::tr_args("settings-reset-a11y", &[("field", tr(label_key))])
        .replace(['\u{2068}', '\u{2069}'], "")
}

fn reset_nodes(nodes: &[Node]) -> Vec<&Node> {
    nodes
        .iter()
        .filter(|n| n.role == Role::Button && n.text.starts_with("Reset "))
        .collect()
}

fn controller_with_device(label: &str) -> (Controller, TempDir) {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "modplayer-ui-settings-reset-{label}-{}-{unique}",
        std::process::id()
    ));
    let _ = std::fs::create_dir_all(&dir);
    let store = SettingsStore::with_path(dir.join("settings.toml"));
    let device = FakeDevice {
        id: DeviceId::new("dev-1").unwrap(),
        name: "Speakers".to_string(),
        rate: SampleRate::new(44_100),
        channels: 2,
        buffer_range: Some((FrameCount::new(32), FrameCount::new(2048))),
        is_default: true,
    };
    let mut controller =
        PlaybackController::new(FakeBackend::new(vec![device]), ScriptedHost::new(), store);
    controller.confirm_device(DeviceId::new("dev-1").unwrap(), BufferPreset::Balanced);
    (controller, TempDir(dir))
}

/// All three owned-by-Reset screens in one fixture.
struct Screens {
    c: Controller,
    cached: AudioSettings,
    playback: PlaybackScreen,
    ctx: Context,
    _dir: TempDir,
}

impl Screens {
    fn new(label: &str) -> Self {
        let (c, dir) = controller_with_device(label);
        let cached = c.settings_store().load().settings;
        let playback = PlaybackScreen::new(&c);
        Self {
            c,
            cached,
            playback,
            ctx: fresh_ctx(),
            _dir: dir,
        }
    }

    /// One frame of `which` with `events`; nodes plus the focused node id.
    fn frame(&mut self, which: &str, events: Vec<egui::Event>) -> (Vec<Node>, Option<u64>) {
        let mut raw = input();
        raw.events = events;
        let Self {
            c,
            cached,
            playback,
            ctx,
            ..
        } = self;
        let mut output = ctx.run_ui(raw, |ui| match which {
            "audio" => {
                let _ = audio::show(ui, c, cached, None);
            }
            "appearance" => appearance::show(ui, c, cached, None),
            _ => playback::show(ui, c, playback, None),
        });
        let update = output.platform_output.accesskit_update.take().unwrap();
        output.drop_without_applying_deltas();
        let nodes = update
            .nodes
            .iter()
            .map(|(id, node)| Node {
                id: id.0,
                role: node.role(),
                text: node
                    .label()
                    .or_else(|| node.value())
                    .unwrap_or_default()
                    .to_string(),
                bounds: node.bounds().map(|b| {
                    Rect::from_min_max(
                        Pos2::new(b.x0 as f32, b.y0 as f32),
                        Pos2::new(b.x1 as f32, b.y1 as f32),
                    )
                }),
            })
            .collect();
        let focused = self.ctx.memory(|m| m.focused()).map(|i| i.value());
        (nodes, focused)
    }

    /// Two settled frames.
    fn settle(&mut self, which: &str) -> (Vec<Node>, Option<u64>) {
        let _ = self.frame(which, vec![]);
        self.frame(which, vec![])
    }

    fn click(&mut self, which: &str, at: Pos2) {
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let _ = self.frame(which, vec![egui::Event::PointerMoved(at)]);
        let _ = self.frame(which, vec![button(true)]);
        let _ = self.frame(which, vec![button(false)]);
    }
}

fn centre(node: &Node) -> Pos2 {
    node.bounds.unwrap().center()
}

/// Change all eight resettable fields away from their defaults.
fn change_everything(s: &mut Screens) {
    s.c.set_ceiling(CeilingDb::new(-3.0));
    s.c.set_nudge_step_ms(40);
    s.c.set_device_name("Studio").unwrap();
    s.c.confirm_device(DeviceId::new("dev-1").unwrap(), BufferPreset::Safe);
    s.c.set_high_contrast(true);
    s.cached.safe_volume = SafeVolume {
        enabled: false,
        cap: VolumePercent::new(30),
    };
    s.cached.theme = Theme::Dark;
    s.cached.high_contrast = true;
    s.playback = PlaybackScreen::new(&s.c);
}

/// F10: at defaults no Reset exists anywhere.
#[test]
fn f10_reset_absent_at_defaults() {
    let mut s = Screens::new("f10");
    for which in ["audio", "appearance", "playback"] {
        let (nodes, _) = s.settle(which);
        assert!(reset_nodes(&nodes).is_empty(), "{which}: {nodes:#?}");
    }
}

/// F10, F11, F14: after changing every field each of the eight offers a
/// Reset with the accessible name "Reset {field} to default"; nothing else
/// does.
#[test]
fn f10_f11_f14_exactly_eight_resets_with_names() {
    let mut s = Screens::new("f11");
    change_everything(&mut s);
    let mut names = Vec::new();
    for which in ["audio", "appearance", "playback"] {
        let (nodes, _) = s.settle(which);
        names.extend(reset_nodes(&nodes).iter().map(|n| n.text.clone()));
    }
    names.sort();
    let mut expected: Vec<String> = [
        "setting-buffer-preset",
        "setting-limiter-ceiling",
        "setting-safe-volume",
        "setting-safe-volume-cap",
        "setting-theme",
        "setting-high-contrast",
        "setting-device-name",
        "setting-nudge-step",
    ]
    .iter()
    .map(|k| reset_name(k))
    .collect();
    expected.sort();
    assert_eq!(names, expected);
}

/// F12, F15: clicking Reset restores only that field; next frame the Reset
/// is gone and the control has focus.
#[test]
fn f12_f15_click_resets_one_field_and_returns_focus() {
    let mut s = Screens::new("f12");
    change_everything(&mut s);
    let (nodes, _) = s.settle("audio");
    let name = reset_name("setting-limiter-ceiling");
    let reset = find(&nodes, &name).clone();
    s.click("audio", centre(&reset));

    let (nodes, focused) = s.settle("audio");
    assert_eq!(s.c.ceiling(), CeilingDb::default());
    assert!(nodes.iter().all(|n| n.text != name), "Reset hides");
    // Every other field keeps its changed value.
    assert_eq!(s.c.nudge_step_ms(), 40);
    assert_eq!(s.c.preset(), BufferPreset::Safe);
    assert!(!s.cached.safe_volume.enabled);
    assert_eq!(s.cached.safe_volume.cap.value(), 30);
    let focused_node = nodes
        .iter()
        .find(|n| Some(n.id) == focused)
        .expect("a node has focus");
    assert!(
        matches!(focused_node.role, Role::Slider | Role::SpinButton),
        "focus returns to the ceiling slider, got {focused_node:?}"
    );
    let ceiling_label = y_of(&nodes, &tr("setting-limiter-ceiling"));
    let cap_label = y_of(&nodes, &tr("setting-safe-volume-cap"));
    let y = focused_node.bounds.unwrap().min.y;
    assert!(
        ceiling_label < y && y < cap_label,
        "it is the ceiling control"
    );
}

/// F12: keyboard activation — Tab from the control reaches its Reset;
/// Enter and Space both activate it.
#[test]
fn f12_keyboard_activation_enter_and_space() {
    for key in [egui::Key::Enter, egui::Key::Space] {
        let mut s = Screens::new("f12-kb");
        s.c.set_nudge_step_ms(40);
        let (nodes, _) = s.settle("playback");
        let reset = find(&nodes, &reset_name("setting-nudge-step")).clone();
        // Tab until the Reset button owns focus (F28: control, then Reset).
        let mut focused = None;
        for _ in 0..12 {
            let tab = egui::Event::Key {
                key: egui::Key::Tab,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            };
            focused = s.frame("playback", vec![tab]).1;
            if focused == Some(reset.id) {
                break;
            }
        }
        assert_eq!(focused, Some(reset.id), "Tab reaches Reset");
        let press = |pressed| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let _ = s.frame("playback", vec![press(true)]);
        let _ = s.frame("playback", vec![press(false)]);
        let _ = s.settle("playback");
        assert_eq!(s.c.nudge_step_ms(), 10, "{key:?} resets the nudge step");
    }
}

/// F13: Reset hides again once the value is back at default by hand; the
/// cap offers Reset even while its switch is off.
#[test]
fn f13_hidden_when_back_at_default_and_cap_offered_with_switch_off() {
    let mut s = Screens::new("f13");
    s.c.set_ceiling(CeilingDb::new(-3.0));
    let name = reset_name("setting-limiter-ceiling");
    assert!(s.settle("audio").0.iter().any(|n| n.text == name));
    s.c.set_ceiling(CeilingDb::default());
    assert!(s.settle("audio").0.iter().all(|n| n.text != name));

    s.cached.safe_volume = SafeVolume {
        enabled: false,
        cap: VolumePercent::new(30),
    };
    let nodes = s.settle("audio").0;
    assert!(
        nodes
            .iter()
            .any(|n| n.text == reset_name("setting-safe-volume-cap")),
        "cap Reset while the switch is off"
    );
}

/// F14: buffer-preset Reset is hidden without a preferred device.
#[test]
fn f14_buffer_preset_reset_needs_a_preferred_device() {
    let (c, dir) = controller("f14");
    let mut cached = c.settings_store().load().settings;
    let mut c = c;
    let ctx = fresh_ctx();
    assert!(c.preferred_device().is_none());
    let _ = frame(&ctx, |ui| {
        let _ = audio::show(ui, &mut c, &mut cached, None);
    });
    let nodes = frame(&ctx, |ui| {
        let _ = audio::show(ui, &mut c, &mut cached, None);
    });
    assert!(reset_nodes(&nodes).is_empty());
    drop(dir);
}

/// F15: resetting the buffer preset writes through `confirm_device`.
#[test]
fn f15_buffer_preset_reset_restores_balanced() {
    let mut s = Screens::new("f15");
    s.c.confirm_device(DeviceId::new("dev-1").unwrap(), BufferPreset::Safe);
    let (nodes, _) = s.settle("audio");
    let reset = find(&nodes, &reset_name("setting-buffer-preset")).clone();
    s.click("audio", centre(&reset));
    assert_eq!(s.c.preset(), BufferPreset::Balanced);
}

// ---------------------------------------------------------------------
// Polish — 40 % expansion (F27, SC-007) and keyboard order (F28)
// ---------------------------------------------------------------------

/// Roles whose nodes are leaf text or controls; containers (groups, cards,
/// scroll areas) legitimately enclose their neighbours and are skipped.
fn is_leaf(role: Role) -> bool {
    matches!(
        role,
        Role::Label
            | Role::Heading
            | Role::Button
            | Role::Slider
            | Role::SpinButton
            | Role::ComboBox
            | Role::Switch
            | Role::CheckBox
            | Role::TextInput
    )
}

/// F27, SC-007: with every string padded by 40 %, no owned label, value,
/// caption, group header or Reset leaves the viewport or overlaps another.
#[test]
fn f27_no_clipping_or_overlap_at_40_percent_expansion() {
    modplayer_core::i18n::with_pseudo_expansion(40, || {
        let mut s = Screens::new("f27");
        // Changed values so every Reset is on screen too.
        change_everything(&mut s);
        let viewport = input().screen_rect.unwrap();
        for which in ["audio", "appearance", "playback"] {
            let (nodes, _) = s.settle(which);
            let leaves: Vec<&Node> = nodes
                .iter()
                .filter(|n| is_leaf(n.role) && n.bounds.is_some_and(|b| b.area() > 0.0))
                .collect();
            assert!(!leaves.is_empty(), "{which}: no nodes");
            for n in &leaves {
                let b = n.bounds.unwrap();
                assert!(
                    b.min.x >= viewport.min.x - 0.5 && b.max.x <= viewport.max.x + 0.5,
                    "{which}: `{}` clipped horizontally: {b:?}",
                    n.text
                );
            }
            for (i, a) in leaves.iter().enumerate() {
                for b in &leaves[i + 1..] {
                    let (ra, rb) = (a.bounds.unwrap(), b.bounds.unwrap());
                    let overlap = ra.intersect(rb);
                    let significant = overlap.width() > 1.0 && overlap.height() > 1.0;
                    // A control may expose a child node with its own text
                    // inside its bounds; only distinct, non-nested nodes
                    // count as overlapping.
                    let nested = ra.contains_rect(rb) || rb.contains_rect(ra);
                    assert!(
                        !significant || nested,
                        "{which}: `{}` overlaps `{}`: {ra:?} vs {rb:?}",
                        a.text,
                        b.text
                    );
                }
            }
        }
    });
}

/// F28: Tab visits each control immediately followed by its own Reset, and
/// groups top to bottom.
#[test]
fn f28_tab_order_is_control_then_reset_top_to_bottom() {
    let mut s = Screens::new("f28");
    change_everything(&mut s);
    let (nodes, _) = s.settle("audio");
    let resets: Vec<Node> = reset_nodes(&nodes).into_iter().cloned().collect();
    assert!(resets.len() >= 3, "audio offers several Resets");

    // Walk the tab cycle once, recording each focused node's position.
    let mut visited: Vec<Node> = Vec::new();
    for _ in 0..40 {
        let tab = egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let (nodes, focused) = s.frame("audio", vec![tab]);
        if let Some(node) = nodes.iter().find(|n| Some(n.id) == focused) {
            if visited.iter().any(|v| v.id == node.id) {
                break; // the cycle wrapped
            }
            visited.push(node.clone());
        }
    }

    // Each Reset directly follows a focusable control that sits on the same
    // row band above it (the control line), never another Reset.
    for reset in &resets {
        let at = visited
            .iter()
            .position(|v| v.id == reset.id)
            .unwrap_or_else(|| panic!("Tab never reached `{}`", reset.text));
        assert!(at > 0, "`{}` cannot be first in order", reset.text);
        let before = &visited[at - 1];
        assert_ne!(before.role, Role::Button, "control precedes its Reset");
        let (bb, rb) = (before.bounds.unwrap(), reset.bounds.unwrap());
        assert!(
            (bb.center().y - rb.center().y).abs() < rb.height().max(bb.height()),
            "control `{}` shares the line with `{}`",
            before.text,
            reset.text
        );
    }

    // Groups top to bottom: one pass through the cycle never moves upward.
    for pair in visited.windows(2) {
        assert!(
            pair[1].bounds.unwrap().min.y >= pair[0].bounds.unwrap().min.y - 1.0,
            "focus order moved upward: {} -> {}",
            pair[0].text,
            pair[1].text
        );
    }
}

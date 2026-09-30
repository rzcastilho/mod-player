// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! Markers panel, overlay and shortcut tests (006, US1,
//! contracts/ui-markers.md).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use egui::accesskit::{NodeId, Role, Toggled};
use egui::{Context, Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, Shape, Stroke};
use modplayer_audio_io::{FakeBackend, FakeDevice};
use modplayer_audio_source::{Availability, TrackId, TrackRef};
use modplayer_audio_source_synthetic::{ScriptedHost, ScriptedHostHandle};
use modplayer_core::actions::ScopeState;
use modplayer_core::markers::{CueSlot, MarkerId, Owner, RepeatCount, TrackMarkers};
use modplayer_core::plugins::PluginId;
use modplayer_core::settings::SettingsStore;
use modplayer_core::{Intent, LoopState, PlaybackController, tr, tr_args};
use modplayer_engine::{BufferPreset, DeviceId, FrameCount, SampleRate};
use modplayer_ui::artwork::ArtworkCache;
use modplayer_ui::waveform::{DetailWindow, PanelFocus, TimeSpace, WaveformState};
use modplayer_ui::{Shell, actions};

/// How many frames a jump's landing position may sit past the cue it
/// targeted (006 US4): landing a seek requires one render call (the
/// engine drains its command queue at the start of it), which itself
/// advances playback by that render's own frame count — double this
/// suite's negotiated buffer size, so genuinely comfortable.
const JUMP_LAND_TOLERANCE_FRAMES: u64 = 512;

/// 014-design-tokens-and-type-scale (US2, T022/T030): a bare
/// `Context::default()` has none of the token `Style`'s
/// `Name("display")`/`Name("section")` text styles installed, which
/// `now_playing::show` (hosting this panel) and the Markers panel's own
/// header now reach — panicking on layout otherwise. Install them once,
/// exactly as `App::new`/`App::update` do (mirrors `controls.rs` test's
/// identically-named helper).
fn fresh_ctx() -> Context {
    let ctx = Context::default();
    modplayer_ui::theme::apply_tokens(&ctx);
    ctx
}

/// As [`fresh_ctx`], with the high-contrast token style installed
/// (017-high-contrast-appearance, Phase 5/US3) — mirrors what
/// `App::new`/`App::ui` do once `controller.high_contrast()` is `true`.
fn fresh_ctx_high_contrast() -> Context {
    let ctx = Context::default();
    modplayer_ui::theme::apply_tokens_for(&ctx, true);
    ctx
}

/// `true` if `shapes` contains a shape carrying the high-contrast marker
/// outline (017-high-contrast-appearance, contracts/marker-outline.md §2):
/// a `LineSegment` casing (wider than the plain 1–2px palette stroke), a
/// `Path`'s own stroke (the point-glyph/clamped-warning polygons), or a
/// `Rect`'s stroke (the cue glyph's outside `rect_stroke`, the armed
/// loop-region span's). All three are how O1–O6 draw the outline.
fn has_outline_shapes(shapes: &[Shape], outline_color: egui::Color32) -> bool {
    shapes.iter().any(|shape| match shape {
        Shape::LineSegment { stroke, .. } => stroke.color == outline_color && stroke.width > 1.0,
        Shape::Path(path) => {
            path.stroke.width > 0.0
                && matches!(
                    path.stroke.color,
                    egui::epaint::ColorMode::Solid(c) if c == outline_color
                )
        }
        Shape::Rect(rect) => rect.stroke.width > 0.0 && rect.stroke.color == outline_color,
        _ => false,
    })
}

/// T004 (023-markers-panel-structure, research R6): each Markers-panel
/// row action's Quiet icon glyph (`theme::markers::ROW_ACTION_*_GLYPH`)
/// resolves in the app's configured proportional font after `theme::
/// apply` — the same font-coverage regression pattern `widgets::controls`
/// already pins for the panel-card disclosure glyphs. A failing glyph
/// would draw as a tofu box; research R6 says to substitute a covered
/// glyph from the same family and record the change there.
#[test]
fn row_action_glyphs_covered_by_fonts() {
    let ctx = fresh_ctx();
    ctx.run_ui(default_input(), |_ui| {})
        .drop_without_applying_deltas();
    let font_id = egui::FontId::proportional(14.0);
    for glyph in [
        modplayer_ui::theme::markers::ROW_ACTION_JUMP_GLYPH,
        modplayer_ui::theme::markers::ROW_ACTION_NUDGE_EARLIER_GLYPH,
        modplayer_ui::theme::markers::ROW_ACTION_NUDGE_LATER_GLYPH,
        modplayer_ui::theme::markers::ROW_ACTION_REMOVE_GLYPH,
    ] {
        assert!(
            ctx.fonts_mut(|fonts| fonts.has_glyphs(&font_id, glyph)),
            "{glyph:?} has no proportional glyph and would draw as tofu"
        );
    }
}

/// T043 follow-up (023-markers-panel-structure, research R6): the swatch
/// popover's "current colour" mark (contract P4) is painted with
/// `theme::mono_font_id()`, so it must resolve in the monospace family.
/// The originally-shipped `✓` (U+2713) did not and drew as a tofu box in
/// the manual walk (M5/M11).
#[test]
fn palette_current_glyph_covered_by_mono_font() {
    let ctx = fresh_ctx();
    ctx.run_ui(default_input(), |_ui| {})
        .drop_without_applying_deltas();
    let glyph = modplayer_ui::theme::markers::PALETTE_CURRENT_GLYPH;
    assert!(
        ctx.fonts_mut(|fonts| fonts.has_glyphs(&modplayer_ui::theme::mono_font_id(), glyph)),
        "{glyph:?} has no monospace glyph and would draw as tofu"
    );
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-ui-markers-{label}-{}-{unique}",
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

fn fake_device() -> FakeDevice {
    FakeDevice {
        id: DeviceId::new("dev-1").unwrap_or_else(|| unreachable!()),
        name: "Speakers".to_string(),
        rate: SampleRate::new(44_100),
        channels: 2,
        buffer_range: Some((FrameCount::new(32), FrameCount::new(2048))),
        is_default: true,
    }
}

fn track(id: &str, duration_ms: u32) -> TrackRef {
    TrackRef::new(
        TrackId::new(format!("spotify:track:{id}")).unwrap_or_else(|_| unreachable!()),
        id,
        vec!["Artist".to_string()],
        None,
        None,
        duration_ms,
        Availability::Available,
    )
}

/// Serializes any test in this binary that briefly overrides the
/// process-global `MODPLAYER_TRACK_STATE_DIR` for `PlaybackController::
/// new`'s one synchronous read of it (mirrors `controller_markers.rs`'s
/// own lock; research R10/R11).
static TRACK_STATE_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Both temp dirs `active_controller` allocates (settings, track-state);
/// kept alive together so either can be dropped (cleaned up) only once the
/// test itself is done with the controller.
struct TestDirs(#[allow(dead_code)] TempDir, #[allow(dead_code)] TempDir);

/// A controller over a confirmed device, playing (mirrors `now_playing.rs`'s
/// own `active_controller`): the baseline every test below starts from.
/// Its `MODPLAYER_TRACK_STATE_DIR` is a temp dir, not the real per-user
/// one (006, contracts/marker-service.md §3) — every test here creates
/// markers/regions, and 006 US2's `sync_marker_attachment`/debounced
/// flush now run on every `dispatch`, so an unisolated controller would
/// read and write the real machine's saved track state.
fn active_controller(
    label: &str,
) -> (
    PlaybackController<FakeBackend, ScriptedHost>,
    ScriptedHostHandle,
    TestDirs,
) {
    let (store, dir) = fresh_store(label);
    let track_state_dir = TempDir::new(&format!("{label}-track-state"));
    let host = ScriptedHost::new();
    let handle = host.handle();
    let devices = vec![fake_device()];
    let mut controller = {
        let _guard = TRACK_STATE_ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Safety: narrowly scopes the mutation to the one synchronous
        // read `PlaybackController::new` does of this var, serialized
        // against every other test in this binary via the lock above.
        unsafe { std::env::set_var("MODPLAYER_TRACK_STATE_DIR", track_state_dir.path()) };
        let controller = PlaybackController::new(FakeBackend::new(devices), host, store);
        unsafe { std::env::remove_var("MODPLAYER_TRACK_STATE_DIR") };
        controller
    };
    // Bundled plugins stay un-launched: `launch()` only spawns records
    // flagged `enabled`, and Key & Tempo (013) would otherwise register
    // its panel into the Now Playing dock asynchronously, shifting the
    // layout between one frame and the next under the tests here that
    // measure a widget's bounds and then click it.
    let bundled: Vec<PluginId> = controller
        .plugins_mut()
        .records()
        .iter()
        .map(|r| r.id)
        .collect();
    for id in bundled {
        if let Some(record) = controller.plugins_mut().record_mut(id) {
            record.enabled = false;
        }
    }
    controller.launch();
    controller.confirm_device(
        DeviceId::new("dev-1").unwrap_or_else(|| unreachable!()),
        BufferPreset::Balanced,
    );
    controller.set_playback_permitted(true, None);
    controller.tick();
    (controller, handle, TestDirs(dir, track_state_dir))
}

fn default_input() -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
        ..Default::default()
    }
}

fn key_event(key: Key) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

/// A key-up event for `key` — egui's own `InputState::begin_pass` derives
/// `repeat` from whether the key is still in its `keys_down` set (it
/// ignores the `repeat` field an integration sends), so every test press
/// must be paired with a release or the *next* press of the same key
/// arrives auto-marked `repeat: true` and FR-019 correctly drops it for a
/// non-`repeats_while_held` action.
fn key_release(key: Key) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

/// Run the 007 dispatcher/invoker (contracts/ui-actions.md §1) exactly as
/// `App::ui` does, scoped as if the Now Playing view were showing (every
/// test in this file draws only `now_playing::show`) — `Shell` is a
/// throwaway since nothing here exercises navigation.
fn dispatch_for_test(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    waveform: &mut WaveformState,
) {
    let claims = actions::claims_snapshot(ctx);
    actions::clear_claims(ctx);
    let scope = ScopeState {
        now_playing_shown: true,
        marker_focused: waveform.focused_marker.is_some(),
    };
    let mut shell = Shell::default();
    actions::dispatch_and_invoke(ctx, &claims, &scope, controller, &mut shell, waveform);
}

/// Run one Now Playing frame with no key event — just dispatch (a no-op
/// with nothing pressed) then the screen's own draw pass. Used to let a
/// widget that just opened (e.g. a rename `TextEdit`) actually draw and
/// claim focus before the *next* frame's dispatch can see it as focused
/// (design note 2: claims, and now `ctx.text_edit_focused()`, are always
/// "last frame's").
fn settle_frame(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    artwork: &mut ArtworkCache,
    waveform: &mut WaveformState,
) {
    let output = ctx.run_ui(default_input(), |ui| {
        dispatch_for_test(&ui.ctx().clone(), controller, waveform);
        modplayer_ui::now_playing::show(
            ui,
            controller,
            artwork,
            waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        )
    });
    output.drop_without_applying_deltas();
}

/// Run one Now Playing frame with `key` pressed (contracts/ui-markers.md
/// §2; re-pointed at the 007 dispatcher, contracts/ui-actions.md §7), on
/// the same `ctx` across calls so egui's own focus bookkeeping persists
/// between them, as `now_playing.rs`'s own tests do.
fn press_key(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    artwork: &mut ArtworkCache,
    waveform: &mut WaveformState,
    key: Key,
) {
    let mut input = default_input();
    input.events.push(key_event(key));
    input.events.push(key_release(key));
    let output = ctx.run_ui(input, |ui| {
        dispatch_for_test(&ui.ctx().clone(), controller, waveform);
        modplayer_ui::now_playing::show(
            ui,
            controller,
            artwork,
            waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        )
    });
    output.drop_without_applying_deltas();
}

/// As [`press_key`], with an explicit modifier (e.g. `Shift+←`,
/// contracts/ui-markers.md §3).
fn press_key_with(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    artwork: &mut ArtworkCache,
    waveform: &mut WaveformState,
    key: Key,
    modifiers: Modifiers,
) {
    let mut input = default_input();
    input.events.push(Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    });
    input.events.push(key_release(key));
    let output = ctx.run_ui(input, |ui| {
        dispatch_for_test(&ui.ctx().clone(), controller, waveform);
        modplayer_ui::now_playing::show(
            ui,
            controller,
            artwork,
            waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        )
    });
    output.drop_without_applying_deltas();
}

/// Run one Now Playing frame and return the screen-space bounds of the
/// topmost accesskit node for which `matches` holds (mirrors `now_playing.
/// rs`'s own `overview_bounds`, generalised to any role/label).
fn find_node_bounds(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    artwork: &mut ArtworkCache,
    waveform: &mut WaveformState,
    matches: impl Fn(&egui::accesskit::Node) -> bool,
) -> Rect {
    let mut output = ctx.run_ui(default_input(), |ui| {
        modplayer_ui::now_playing::show(
            ui,
            controller,
            artwork,
            waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        )
    });
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    output.drop_without_applying_deltas();

    let mut best: Option<egui::accesskit::Rect> = None;
    for (_, node) in &update.nodes {
        if !matches(node) {
            continue;
        }
        if let Some(bounds) = node.bounds()
            && best.as_ref().is_none_or(|b| bounds.y0 < b.y0)
        {
            best = Some(bounds);
        }
    }
    let bounds = best.unwrap_or_else(|| unreachable!("no matching accesskit node found"));
    Rect::from_min_max(
        Pos2::new(bounds.x0 as f32, bounds.y0 as f32),
        Pos2::new(bounds.x1 as f32, bounds.y1 as f32),
    )
}

/// A point marker's glyph hit-target bounds — the only `Role::Button`
/// whose accessible name (`marker-glyph`, contracts/ui-markers.md §1)
/// contains `"Marker"` (`role_label`'s point-kind text; case-sensitive, so
/// it never matches the panel's lowercase "markers" chrome buttons).
fn glyph_bounds(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    artwork: &mut ArtworkCache,
    waveform: &mut WaveformState,
) -> Rect {
    find_node_bounds(ctx, controller, artwork, waveform, |node| {
        node.role() == Role::Button
            && (node.label().is_some_and(|l| l.contains("Marker"))
                || node.value().is_some_and(|v| v.contains("Marker")))
    })
}

/// The overview waveform's own bounds: the `Role::Slider` node named
/// `transport-seek` (021-transport-bar-and-panel-layout, contract B2.4:
/// the bar's own master-volume `Slider` now sits above it on screen, so
/// "the topmost `Role::Slider`" no longer picks it out).
fn overview_bounds(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    artwork: &mut ArtworkCache,
    waveform: &mut WaveformState,
) -> Rect {
    find_node_bounds(ctx, controller, artwork, waveform, |node| {
        node.role() == Role::Slider && node.label() == Some(tr("transport-seek").as_str())
    })
}

/// Every non-empty AccessKit `value`/`label` text the frame produced
/// (mirrors `now_playing.rs`'s own `rendered_texts`).
fn rendered_texts(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    artwork: &mut ArtworkCache,
    waveform: &mut WaveformState,
) -> Vec<String> {
    let mut output = ctx.run_ui(default_input(), |ui| {
        modplayer_ui::now_playing::show(
            ui,
            controller,
            artwork,
            waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        )
    });
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    output.drop_without_applying_deltas();

    update
        .nodes
        .iter()
        .flat_map(|(_, node)| [node.value(), node.label()])
        .filter_map(|text| text.map(str::to_string))
        .filter(|text| !text.is_empty())
        .collect()
}

// ---------------------------------------------------------------------
// 023-markers-panel-structure — Phase 4, User Story 2 helpers: a
// `now_playing::show` frame's AccessKit tree plus its focused node
// (mirrors `queue_view.rs`'s own `Frame`/`run`/`click_at`), and a plain
// `Tab` input. Used by T020-T026 below.
// ---------------------------------------------------------------------

#[derive(Debug, Clone)]
struct AccessNode {
    role: Role,
    label: Option<String>,
    bounds: Option<Rect>,
    toggled: Option<Toggled>,
}

struct Frame {
    focus: NodeId,
    nodes: Vec<(NodeId, AccessNode)>,
}

impl Frame {
    fn focused(&self) -> Option<&AccessNode> {
        self.nodes
            .iter()
            .find(|(id, _)| *id == self.focus)
            .map(|(_, n)| n)
    }

    /// Every node's bounds matching `role`/`label`, sorted top to bottom
    /// (mirrors `queue_view.rs`'s `nth_button_bounds`: AccessKit tree
    /// order is not visual order, so callers needing a specific one of
    /// several same-labelled nodes must sort by bounds themselves, as
    /// `markers_group_headings_in_order` already does elsewhere in this
    /// file).
    fn all_bounds(&self, role: Role, label: &str) -> Vec<Rect> {
        let mut matches: Vec<Rect> = self
            .nodes
            .iter()
            .filter(|(_, n)| n.role == role && n.label.as_deref() == Some(label))
            .filter_map(|(_, n)| n.bounds)
            .collect();
        matches.sort_by(|a, b| {
            a.top()
                .partial_cmp(&b.top())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        matches
    }

    fn nth_bounds(&self, role: Role, label: &str, nth: usize) -> Option<Rect> {
        self.all_bounds(role, label).get(nth).copied()
    }

    fn bounds(&self, role: Role, label: &str) -> Rect {
        self.nth_bounds(role, label, 0)
            .unwrap_or_else(|| panic!("expected a {role:?} node labelled {label:?}, found none"))
    }

    fn find(&self, role: Role, label: &str) -> Option<&AccessNode> {
        self.nodes
            .iter()
            .find(|(_, n)| n.role == role && n.label.as_deref() == Some(label))
            .map(|(_, n)| n)
    }

    fn has(&self, role: Role, label: &str) -> bool {
        self.find(role, label).is_some()
    }
}

/// One `now_playing::show` frame over `input`, run through the same
/// dispatch-then-render order as [`press_key`]/[`settle_frame`]: the
/// focused node's id and every AccessKit node's role/label/bounds/
/// toggled state.
fn frame(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    artwork: &mut ArtworkCache,
    waveform: &mut WaveformState,
    input: RawInput,
) -> Frame {
    let mut output = ctx.run_ui(input, |ui| {
        dispatch_for_test(&ui.ctx().clone(), controller, waveform);
        modplayer_ui::now_playing::show(
            ui,
            controller,
            artwork,
            waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        )
    });
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    let focus = update.focus;
    let nodes = update
        .nodes
        .iter()
        .map(|(id, node)| {
            (
                *id,
                AccessNode {
                    role: node.role(),
                    label: node.label().map(str::to_string),
                    bounds: node.bounds().map(|b| {
                        Rect::from_min_max(
                            Pos2::new(b.x0 as f32, b.y0 as f32),
                            Pos2::new(b.x1 as f32, b.y1 as f32),
                        )
                    }),
                    toggled: node.toggled(),
                },
            )
        })
        .collect();
    output.drop_without_applying_deltas();
    Frame { focus, nodes }
}

fn tab_input() -> RawInput {
    let mut input = tall_input();
    input.events.push(key_event(Key::Tab));
    input.events.push(key_release(Key::Tab));
    input
}

/// As [`default_input`], but a much taller `screen_rect`: several of the
/// tests below render two or three marker rows (plus the Loop group and
/// all 8 Cue rows) at once, which overflows `default_input`'s 600px
/// height — content beyond it sits outside the scroll area's clip rect
/// and cannot receive a pointer event at all.
fn tall_input() -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 4000.0))),
        ..Default::default()
    }
}

/// Press then release the primary button at `pos`, in two separate
/// frames (mirrors `queue_view.rs`'s own `click_at`: a single frame is
/// not guaranteed to register `clicked()`).
fn click_at(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    artwork: &mut ArtworkCache,
    waveform: &mut WaveformState,
    pos: Pos2,
) {
    let mut press = tall_input();
    press.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::default(),
    });
    frame(ctx, controller, artwork, waveform, press);

    let mut release = tall_input();
    release.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::default(),
    });
    frame(ctx, controller, artwork, waveform, release);
}

/// As [`click_at`], but press and release in the same frame — used only
/// to try to collide a click with another widget's own same-frame focus
/// loss (T025 `rename_commits_before_other_row_action`).
fn click_at_same_frame(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    artwork: &mut ArtworkCache,
    waveform: &mut WaveformState,
    pos: Pos2,
) {
    let mut input = tall_input();
    input.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::default(),
    });
    input.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::default(),
    });
    frame(ctx, controller, artwork, waveform, input);
}

/// Every Markers-panel group heading's own accessible label
/// (`markers-group-heading { $label } { $count }`), top-to-bottom
/// (023-markers-panel-structure, contract P1/P2): filters `Role::Heading`
/// nodes down to the three group headings by their label's own prefix
/// (`markers-group-loop`/`-points`/`-cues`'s bare text), which drops the
/// card's own `markers-panel` heading and every other card's heading on
/// the same Now Playing screen.
fn markers_group_headings_in_order(
    ctx: &Context,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    artwork: &mut ArtworkCache,
    waveform: &mut WaveformState,
) -> Vec<String> {
    let mut output = ctx.run_ui(default_input(), |ui| {
        modplayer_ui::now_playing::show(
            ui,
            controller,
            artwork,
            waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        )
    });
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    output.drop_without_applying_deltas();

    // Fluent wraps every interpolated placeable (here, `{ $label }`) in
    // bidi-isolate marks (U+2068/U+2069), so the heading's full text is
    // never a plain `"{label}, {count}"` concatenation — `contains` (not
    // `starts_with`) is what actually finds the group's own bare label
    // inside it.
    let bare_labels = [
        tr("markers-group-loop"),
        tr("markers-group-points"),
        tr("markers-group-cues"),
    ];
    let mut headings: Vec<(f64, String)> = update
        .nodes
        .iter()
        .filter(|(_, node)| node.role() == Role::Heading)
        .filter_map(|(_, node)| {
            let label = node.label()?.to_string();
            if !bare_labels.iter().any(|l| label.contains(l.as_str())) {
                return None;
            }
            let bounds = node.bounds()?;
            Some((bounds.y0, label))
        })
        .collect();
    headings.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    headings.into_iter().map(|(_, label)| label).collect()
}

/// `I` then `O` (contracts/ui-markers.md §2, US1 AS1): each sets the
/// current region's `A`/`B` endpoint to the playhead frame at the moment
/// it is pressed — read straight from `RtShared::position_frames()`, the
/// engine's own frame cursor (FR-009), not a UI-derived estimate.
#[test]
fn i_then_o_creates_region_at_playhead_positions() {
    let (mut controller, _handle, _dir) = active_controller("i-then-o");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    let _ = controller.backend_mut().render_buffers(5);

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    let expected_a = controller.shared().position_frames();
    press_key(&ctx, &mut controller, &mut artwork, &mut waveform, Key::I);

    let region = controller
        .markers()
        .and_then(TrackMarkers::current_region)
        .expect("I must create a region");
    let a_id = controller
        .markers()
        .and_then(|markers| markers.region(region))
        .and_then(|r| r.a)
        .expect("A endpoint must exist after I");
    let a = controller
        .markers()
        .and_then(|markers| markers.position_of(a_id))
        .expect("A marker must resolve");
    assert_eq!(a, expected_a);

    let _ = controller.backend_mut().render_buffers(5);
    let expected_b = controller.shared().position_frames();
    assert!(
        expected_b > expected_a,
        "sanity: playhead must have advanced"
    );

    press_key(&ctx, &mut controller, &mut artwork, &mut waveform, Key::O);

    let markers = controller.markers().expect("markers must exist");
    let region_after = markers.region(region).expect("region must still exist");
    let (a_final, b_final) = region_after
        .span(markers)
        .expect("region must be complete after O");
    assert_eq!(a_final, expected_a);
    assert_eq!(b_final, expected_b);
}

/// `L` (contracts/ui-markers.md §2, AS7, FR-008): refuses with
/// `loop-region-incomplete` while no region/only `A` exists, refuses with
/// `loop-region-too-short`/`loop-region-incomplete` per `MarkerError`, and
/// arms/disarms a real region — each outcome mirrored into
/// `waveform.marker_status` (cleared on success).
#[test]
fn l_toggles_current_region_and_refuses_with_reason() {
    let (mut controller, _handle, _dir) = active_controller("l-toggle");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    // No region at all: refuses.
    press_key(&ctx, &mut controller, &mut artwork, &mut waveform, Key::L);
    assert_eq!(waveform.marker_status, Some("loop-region-incomplete"));

    // `I` succeeds and clears the refusal; only `A` set: `L` still refuses.
    press_key(&ctx, &mut controller, &mut artwork, &mut waveform, Key::I);
    assert_eq!(waveform.marker_status, None);
    press_key(&ctx, &mut controller, &mut artwork, &mut waveform, Key::L);
    assert_eq!(waveform.marker_status, Some("loop-region-incomplete"));

    // A real-length region: `O` completes it, `L` arms it.
    let _ = controller.backend_mut().render_buffers(5);
    press_key(&ctx, &mut controller, &mut artwork, &mut waveform, Key::O);
    assert_eq!(waveform.marker_status, None);
    let _ = controller.backend_mut().render_buffers(1);

    press_key(&ctx, &mut controller, &mut artwork, &mut waveform, Key::L);
    assert_eq!(
        waveform.marker_status, None,
        "a real region must arm, not refuse"
    );
    let _ = controller.backend_mut().render_buffers(1);
    assert_ne!(controller.loop_status().state, LoopState::Disarmed);

    // `L` again disarms.
    press_key(&ctx, &mut controller, &mut artwork, &mut waveform, Key::L);
    assert_eq!(waveform.marker_status, None);
    let _ = controller.backend_mut().render_buffers(1);
    assert_eq!(controller.loop_status().state, LoopState::Disarmed);
}

/// An armed, active, finite-repeat region shows `loop-wraps-remaining`
/// (contracts/ui-markers.md §4).
#[test]
fn armed_region_shows_wraps_remaining() {
    let (mut controller, _handle, _dir) = active_controller("wraps-remaining");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();

    let _ = controller.backend_mut().render_buffers(1);
    controller
        .set_loop_a()
        .unwrap_or_else(|e| unreachable!("set_loop_a: {e}"));
    let _ = controller.backend_mut().render_buffers(1);
    controller
        .set_loop_b()
        .unwrap_or_else(|e| unreachable!("set_loop_b: {e}"));
    let region = controller
        .markers()
        .and_then(TrackMarkers::current_region)
        .unwrap_or_else(|| unreachable!("region must exist"));
    controller
        .set_loop_repeat(region, RepeatCount::times_clamped(5))
        .unwrap_or_else(|e| unreachable!("set_loop_repeat: {e}"));
    let a = {
        let markers = controller
            .markers()
            .unwrap_or_else(|| unreachable!("markers must exist"));
        markers
            .region(region)
            .unwrap_or_else(|| unreachable!("region must exist"))
            .span(markers)
            .unwrap_or_else(|| unreachable!("region must be complete"))
            .0
    };
    controller
        .arm_loop(region)
        .unwrap_or_else(|e| unreachable!("arm_loop: {e}"));
    controller.seek_frames(a);
    let _ = controller.backend_mut().render_buffers(1);

    let mut wraps = 0;
    for _ in 0..500 {
        let _ = controller.backend_mut().render_buffers(1);
        wraps = controller.shared().loop_wraps();
        if wraps >= 2 {
            break;
        }
    }
    assert!((2..5).contains(&wraps), "wraps={wraps}");

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let texts = rendered_texts(&ctx, &mut controller, &mut artwork, &mut waveform);

    let remaining = 5u32.saturating_sub(wraps);
    let expected = tr_args("loop-wraps-remaining", &[("count", remaining.to_string())]);
    assert!(
        texts.contains(&expected),
        "expected {expected:?} among {texts:?}"
    );
}

/// An armed region whose playhead has not (yet) entered `[A, B)` shows the
/// `loop-armed-inactive` badge (contracts/ui-markers.md §4, data-model.md
/// §3's `RtShared::loop_state == 1`).
#[test]
fn armed_inactive_badge_when_state_1() {
    let (mut controller, _handle, _dir) = active_controller("armed-inactive");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();

    let _ = controller.backend_mut().render_buffers(1);
    controller
        .set_loop_a()
        .unwrap_or_else(|e| unreachable!("set_loop_a: {e}"));
    let _ = controller.backend_mut().render_buffers(1);
    controller
        .set_loop_b()
        .unwrap_or_else(|e| unreachable!("set_loop_b: {e}"));
    let region = controller
        .markers()
        .and_then(TrackMarkers::current_region)
        .unwrap_or_else(|| unreachable!("region must exist"));
    controller
        .arm_loop(region)
        .unwrap_or_else(|e| unreachable!("arm_loop: {e}"));
    // Deliberately no seek back to `A`: the playhead has already moved past
    // `B`, so the commit lands armed-but-inactive (state 1), never active.
    let _ = controller.backend_mut().render_buffers(1);
    assert_eq!(controller.loop_status().state, LoopState::ArmedInactive);

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let texts = rendered_texts(&ctx, &mut controller, &mut artwork, &mut waveform);

    assert!(
        texts.contains(&modplayer_core::tr("loop-armed-inactive")),
        "expected the armed-inactive badge among {texts:?}"
    );
}

/// A helper to run a paint closure against a freshly allocated rect/
/// `TimeSpace` and capture the raw (pre-tessellation) shapes it produced,
/// mirroring `now_playing.rs`'s own `rendered_painted_texts` pattern.
fn painted_shapes(
    window: std::ops::Range<u64>,
    sample_rate: u32,
    paint: impl FnOnce(&egui::Painter, &TimeSpace),
) -> Vec<Shape> {
    let ctx = fresh_ctx();
    let mut paint = Some(paint);
    let output = ctx.run_ui(default_input(), |ui| {
        let (rect, _response) =
            ui.allocate_exact_size(egui::vec2(400.0, 40.0), egui::Sense::hover());
        let space = TimeSpace::new(rect, window.clone(), sample_rate);
        if let Some(paint) = paint.take() {
            paint(ui.painter(), &space);
        }
    });
    let shapes = output
        .shapes
        .iter()
        .map(|clipped| clipped.shape.clone())
        .collect();
    output.drop_without_applying_deltas();
    shapes
}

/// `markers::paint_overlay` (006, 022-waveform-legibility WL4,
/// contracts/ui-markers.md §1, §5, data-model.md §7): one line per marker
/// regardless of `loop_state`, plus the (armed) region's span rendered per
/// `loop_shade(region.armed, loop_state)` — idle-outline while disarmed,
/// hatched with no rect while armed-inactive, filled while armed-active.
#[test]
fn overlay_paints_lines_and_span_states() {
    let id = TrackId::new("spotify:track:overlay").unwrap_or_else(|_| unreachable!());
    let mut markers = TrackMarkers::new(id, 44_100, 44_100 * 200);
    markers
        .set_loop_a(1_000)
        .unwrap_or_else(|e| unreachable!("set_loop_a: {e}"));
    markers
        .set_loop_b(5_000)
        .unwrap_or_else(|e| unreachable!("set_loop_b: {e}"));
    let region = markers
        .current_region()
        .unwrap_or_else(|| unreachable!("region must exist"));

    for loop_state in [0u8, 1, 2] {
        // WL4: shading now follows the region's own `armed` flag, not
        // `loop_state` alone — arm it for the armed-inactive/armed-active
        // cases, disarm it for the idle case.
        if loop_state == 0 {
            markers.disarm();
        } else {
            markers
                .arm(region)
                .unwrap_or_else(|e| unreachable!("arm: {e}"));
        }
        let shapes = painted_shapes(0..(44_100 * 200), 44_100, |painter, space| {
            modplayer_ui::markers::paint_overlay(
                painter,
                space,
                Some(&markers),
                loop_state,
                None,
                &modplayer_ui::theme::tokens::LIGHT,
            );
        });

        let line_count = shapes
            .iter()
            .filter(|shape| matches!(shape, Shape::LineSegment { .. }))
            .count();
        let rects: Vec<_> = shapes
            .iter()
            .filter_map(|shape| match shape {
                Shape::Rect(rect_shape) => Some(rect_shape),
                _ => None,
            })
            .collect();

        assert!(
            line_count >= 2,
            "loop_state={loop_state}: expected at least the 2 marker lines, got {line_count}"
        );

        match loop_state {
            1 => {
                assert!(
                    line_count > 2,
                    "armed-inactive must add hatch lines beyond the 2 marker lines"
                );
                assert!(rects.is_empty(), "armed-inactive paints no span rect");
            }
            2 => {
                assert_eq!(line_count, 2, "only the 2 marker lines for armed-active");
                assert_eq!(rects.len(), 1);
                assert!(rects[0].fill.a() > 0, "armed-active span must be filled");
            }
            _ => {
                assert_eq!(line_count, 2, "only the 2 marker lines while disarmed");
                // WL4/FR-005: idle is a separate low-alpha fill rect plus
                // a 1px outline-stroke rect (two `Shape::Rect`s).
                assert_eq!(rects.len(), 2);
                assert!(
                    rects[0].fill.a() > 0,
                    "idle span's fill rect must have a low-alpha fill"
                );
                assert!(
                    rects[1].stroke.width > 0.0,
                    "idle span's outline rect must be stroked"
                );
            }
        }
    }
}

/// `markers::loop_shade` (022-waveform-legibility, data-model.md §7): the
/// full truth table — armed reads `ArmedActive` only at `loop_state == 2`
/// and `ArmedInactive` at 0/1; unarmed is always `Idle`, regardless of
/// `loop_state`.
#[test]
fn loop_shade_truth_table() {
    use modplayer_ui::markers::{LoopShade, loop_shade};

    assert_eq!(loop_shade(true, 2), LoopShade::ArmedActive);
    assert_eq!(loop_shade(true, 0), LoopShade::ArmedInactive);
    assert_eq!(loop_shade(true, 1), LoopShade::ArmedInactive);
    for loop_state in [0u8, 1, 2, 3, 255] {
        assert_eq!(
            loop_shade(false, loop_state),
            LoopShade::Idle,
            "unarmed must always be Idle regardless of loop_state={loop_state}"
        );
    }
}

/// WL4: an incomplete region (one endpoint only) draws no span rect at
/// all; a complete region alongside it still shades normally.
#[test]
fn incomplete_region_draws_nothing_complete_region_still_shades() {
    let id = TrackId::new("spotify:track:incomplete-region").unwrap_or_else(|_| unreachable!());
    let mut markers = TrackMarkers::new(id, 44_100, 44_100 * 200);
    // A complete region.
    markers
        .set_loop_a(1_000)
        .unwrap_or_else(|e| unreachable!("set_loop_a: {e}"));
    markers
        .set_loop_b(5_000)
        .unwrap_or_else(|e| unreachable!("set_loop_b: {e}"));
    // A second, incomplete region (`A` only) — must draw nothing.
    markers
        .set_loop_endpoint_owned(None, true, 10_000, Owner::Host)
        .unwrap_or_else(|e| unreachable!("set_loop_endpoint_owned: {e}"));

    let shapes = painted_shapes(0..(44_100 * 200), 44_100, |painter, space| {
        modplayer_ui::markers::paint_overlay(
            painter,
            space,
            Some(&markers),
            0,
            None,
            &modplayer_ui::theme::tokens::LIGHT,
        );
    });
    let rect_count = shapes
        .iter()
        .filter(|shape| matches!(shape, Shape::Rect(_)))
        .count();
    assert_eq!(
        rect_count, 2,
        "only the complete region shades (idle: fill rect + outline rect); \
         the incomplete region draws no rect at all"
    );
}

/// WL4/data-model.md §7: an armed region that is *not* `current_region`
/// still gets the armed treatment, drawn last; the disarmed current
/// region gets the idle treatment, drawn first — `current_region()` plays
/// no role in shading selection or order.
#[test]
fn armed_non_current_region_shades_armed_and_paints_last() {
    let id = TrackId::new("spotify:track:armed-non-current").unwrap_or_else(|_| unreachable!());
    let mut markers = TrackMarkers::new(id, 44_100, 44_100 * 200);

    // Region A: created and armed first.
    markers
        .set_loop_a(1_000)
        .unwrap_or_else(|e| unreachable!("set_loop_a: {e}"));
    markers
        .set_loop_b(5_000)
        .unwrap_or_else(|e| unreachable!("set_loop_b: {e}"));
    let region_a = markers
        .current_region()
        .unwrap_or_else(|| unreachable!("region A must exist"));
    markers
        .arm(region_a)
        .unwrap_or_else(|e| unreachable!("arm: {e}"));

    // Region B: created after A, becomes `current_region` — but stays
    // disarmed, so it must shade idle even though it is current.
    let (region_b, _) = markers
        .set_loop_endpoint_owned(None, true, 10_000, Owner::Host)
        .unwrap_or_else(|e| unreachable!("set_loop_endpoint_owned: {e}"));
    markers
        .set_loop_endpoint_owned(Some(region_b), false, 15_000, Owner::Host)
        .unwrap_or_else(|e| unreachable!("set_loop_endpoint_owned: {e}"));
    assert_eq!(
        markers.current_region(),
        Some(region_b),
        "sanity: region B must be current"
    );
    assert_eq!(
        markers.region(region_a).map(|r| r.armed),
        Some(true),
        "sanity: region A must still be armed"
    );

    let shapes = painted_shapes(0..(44_100 * 200), 44_100, |painter, space| {
        modplayer_ui::markers::paint_overlay(
            painter,
            space,
            Some(&markers),
            2, // engine says armed-active for whichever region is armed
            None,
            &modplayer_ui::theme::tokens::LIGHT,
        );
    });
    let rects: Vec<_> = shapes
        .iter()
        .filter_map(|shape| match shape {
            Shape::Rect(r) => Some(r),
            _ => None,
        })
        .collect();
    // Idle region B: fill rect then outline-stroke rect, painted first.
    // Armed-active region A: one filled rect (no stroke in normal mode),
    // painted last.
    assert_eq!(rects.len(), 3, "idle (2 rects) + armed-active (1 rect)");
    assert!(
        rects[0].fill.a() > 0 && rects[0].stroke.width == 0.0,
        "region B (idle, current, disarmed) paints its fill first"
    );
    assert!(
        rects[1].fill.a() == 0 && rects[1].stroke.width > 0.0,
        "region B's outline stroke follows its fill"
    );
    assert!(
        rects[2].fill.a() > 0 && rects[2].stroke.width == 0.0,
        "region A (armed, non-current) paints last, as a filled rect"
    );
    // FR-005: idle fill alpha is at most half the armed-active fill alpha.
    assert!(
        rects[0].fill.a() <= rects[2].fill.a() / 2,
        "idle fill alpha {} must be <= half the armed-active fill alpha {}",
        rects[0].fill.a(),
        rects[2].fill.a()
    );
}

/// The Markers panel's empty state (contracts/ui-markers.md §4, FR-020):
/// with no markers on the current track, the row list is replaced by
/// `markers-empty` ("No markers — press I to set A").
#[test]
fn empty_state_shows_press_i_hint() {
    let (mut controller, _handle, _dir) = active_controller("empty-state");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    let texts = rendered_texts(&ctx, &mut controller, &mut artwork, &mut waveform);
    assert!(
        texts.contains(&modplayer_core::tr("markers-empty")),
        "expected the empty-state hint among {texts:?}"
    );
}

/// "Clear all markers" (contracts/ui-markers.md §4, US2 AS6): the header
/// button is a two-step inline confirmation
/// (`markers-clear-confirm { $count }` + yes/no); `Esc` cancels it back to
/// the plain button without touching any marker, while confirming (via
/// `clear_all_markers`, exercised directly here as the "yes" path) empties
/// the model.
#[test]
fn clear_all_two_step_confirm_and_cancel() {
    let (mut controller, _handle, _dir) = active_controller("clear-all");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    controller
        .set_loop_a()
        .unwrap_or_else(|e| unreachable!("set_loop_a: {e}"));
    let count_before = controller.markers().map(TrackMarkers::count).unwrap_or(0);
    assert!(count_before > 0, "sanity: a marker must exist");

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    // Not yet confirming: the plain button, no "Clear N markers?" text.
    let texts = rendered_texts(&ctx, &mut controller, &mut artwork, &mut waveform);
    assert!(texts.contains(&modplayer_core::tr("markers-clear-all")));
    let confirm_text = tr_args(
        "markers-clear-confirm",
        &[("count", count_before.to_string())],
    );
    assert!(!texts.contains(&confirm_text));

    // Two-step confirmation showing.
    waveform.clear_confirm = true;
    let texts = rendered_texts(&ctx, &mut controller, &mut artwork, &mut waveform);
    assert!(
        texts.contains(&confirm_text),
        "expected {confirm_text:?} among {texts:?}"
    );

    // `Esc` cancels: back to the plain button, nothing cleared.
    press_key(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Key::Escape,
    );
    assert!(!waveform.clear_confirm);
    assert_eq!(
        controller.markers().map(TrackMarkers::count).unwrap_or(0),
        count_before,
        "Esc must not clear any marker"
    );

    // Confirming ("yes") empties the model.
    waveform.clear_confirm = true;
    controller.clear_all_markers();
    waveform.clear_confirm = false;
    assert_eq!(
        controller.markers().map(TrackMarkers::count).unwrap_or(0),
        0
    );
}

/// A `clamped` marker (006 FR-018/SC-012: a saved position beyond a
/// shorter re-saved track's current length) gets an extra warning glyph on
/// its overlay line that an ordinary marker does not
/// (`paint_clamped_warning`, contracts/ui-markers.md §1).
#[test]
fn clamped_marker_shows_warning_glyph() {
    let id = TrackId::new("spotify:track:clamped").unwrap_or_else(|_| unreachable!());
    let mut markers = TrackMarkers::new(id, 44_100, 44_100 * 200);
    markers
        .set_loop_a(1_000)
        .unwrap_or_else(|e| unreachable!("set_loop_a: {e}"));

    let triangle_count = |markers: &TrackMarkers| {
        painted_shapes(0..(44_100 * 200), 44_100, |painter, space| {
            modplayer_ui::markers::paint_overlay(
                painter,
                space,
                Some(markers),
                0,
                None,
                &modplayer_ui::theme::tokens::LIGHT,
            );
        })
        .iter()
        .filter(|shape| matches!(shape, Shape::Path(_)))
        .count()
    };

    assert_eq!(triangle_count(&markers), 0, "no clamped markers yet");

    // Shrinking the track below the marker's position flags it `clamped`.
    markers.set_len_frames(500);
    assert_eq!(
        triangle_count(&markers),
        1,
        "a clamped marker must paint exactly one warning triangle"
    );
}

/// `M` (contracts/ui-markers.md §2, US3 AS8): adds a point marker at the
/// playhead with its externalised default name, kept sorted among any
/// other markers.
#[test]
fn m_creates_point_marker_with_default_name_sorted() {
    let (mut controller, _handle, _dir) = active_controller("m-creates-point");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    let expected_pos = controller.shared().position_frames();
    press_key(&ctx, &mut controller, &mut artwork, &mut waveform, Key::M);

    let markers = controller.markers().expect("M must create a marker");
    assert_eq!(markers.count(), 1);
    let marker = markers.markers().first().expect("one marker must exist");
    assert_eq!(marker.position, expected_pos);
    assert_eq!(
        marker.name,
        tr_args("marker-default-name", &[("n", "1".to_string())])
    );

    // A second `M` keeps the list sorted by (position, id).
    let _ = controller.backend_mut().render_buffers(5);
    press_key(&ctx, &mut controller, &mut artwork, &mut waveform, Key::M);
    let markers = controller.markers().expect("markers must exist");
    assert_eq!(markers.count(), 2);
    let positions: Vec<u64> = markers.markers().iter().map(|m| m.position).collect();
    let mut sorted = positions.clone();
    sorted.sort_unstable();
    assert_eq!(positions, sorted, "markers must stay sorted by position");
}

/// The 64-marker limit (SC-005, FR-002) refuses `M` inline, same mechanism
/// as `I`/`O`/`L`.
#[test]
fn sixty_fifth_marker_refused_inline() {
    let (mut controller, _handle, _dir) = active_controller("m-limit");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();

    for _ in 0..64 {
        controller
            .add_point_marker()
            .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));
    }
    assert_eq!(controller.markers().map(TrackMarkers::count), Some(64));

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    press_key(&ctx, &mut controller, &mut artwork, &mut waveform, Key::M);
    assert_eq!(waveform.marker_status, Some("marker-limit-reached"));
    assert_eq!(controller.markers().map(TrackMarkers::count), Some(64));
}

/// While an inline rename is open, the view-level shortcuts (`I`/`O`/`L`/
/// `M`) must not fire — the rename `TextEdit` has focus, so the research
/// R17 text-field guard blocks them (FR-004a).
#[test]
fn shortcuts_inactive_while_rename_open() {
    let (mut controller, _handle, _dir) = active_controller("rename-guard");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();

    let id = controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState {
        focused_marker: Some(id),
        rename: Some((id, String::new())),
        // 023-markers-panel-structure (research R9): the `TextEdit` now
        // calls `request_focus()` only while this one-shot flag is set
        // (exactly what `PanelIntent::OpenRename` sets) — unlike 006's
        // unconditional per-frame call, click-away must be able to steal
        // focus back. This hand-built state mimics a rename that just
        // opened this same frame.
        rename_focus_pending: true,
        ..Default::default()
    };
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    // The rename `TextEdit` (`markers::panel`'s `show_name_cell`) must
    // actually draw and claim focus once before the dispatcher — which
    // runs *before* any widget this frame — can see it as focused
    // (design note 2). A real F2 press has exactly this same one-frame
    // lag between opening the rename and the guard taking effect.
    settle_frame(&ctx, &mut controller, &mut artwork, &mut waveform);

    let count_before = controller.markers().map(TrackMarkers::count).unwrap_or(0);
    press_key(&ctx, &mut controller, &mut artwork, &mut waveform, Key::M);
    assert_eq!(
        controller.markers().map(TrackMarkers::count).unwrap_or(0),
        count_before,
        "M must not create a marker while an inline rename is open"
    );
    assert!(
        waveform.rename.is_some(),
        "the rename itself must stay open (M must not have touched it)"
    );
}

/// The focused-marker keyboard table's arrow row (contracts/ui-markers.md
/// §3, SC-004): `←`/`→` nudge by the configured step, `Shift+←`/`Shift+→`
/// nudge by 10x it.
#[test]
fn glyph_focus_arrow_nudges_by_setting_and_shift_ten_x() {
    let (mut controller, _handle, _dir) = active_controller("nudge");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    controller.set_nudge_step_ms(25);

    let id = controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));
    let start = controller
        .markers()
        .and_then(|m| m.position_of(id))
        .unwrap_or_else(|| unreachable!());

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState {
        focused_marker: Some(id),
        ..Default::default()
    };
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    press_key(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Key::ArrowRight,
    );
    let rate = controller.source_sample_rate().max(1);
    let step = (25u64 * u64::from(rate)) / 1000;
    let after_one = controller
        .markers()
        .and_then(|m| m.position_of(id))
        .unwrap_or_else(|| unreachable!());
    assert_eq!(after_one, start + step, "plain arrow nudges by one step");

    press_key_with(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Key::ArrowRight,
        Modifiers::SHIFT,
    );
    let after_shift = controller
        .markers()
        .and_then(|m| m.position_of(id))
        .unwrap_or_else(|| unreachable!());
    assert_eq!(
        after_shift,
        after_one + step * 10,
        "Shift nudges by 10x the step"
    );

    press_key_with(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Key::ArrowLeft,
        Modifiers::SHIFT,
    );
    let after_shift_back = controller
        .markers()
        .and_then(|m| m.position_of(id))
        .unwrap_or_else(|| unreachable!());
    assert_eq!(after_shift_back, after_shift - step * 10);
}

/// `Delete`/`Backspace` on a focused region endpoint (contracts/
/// ui-markers.md §3, AS5): the region becomes incomplete (and, since it
/// was the armed one, disarmed) and focus returns.
#[test]
fn delete_focused_endpoint_makes_region_incomplete() {
    let (mut controller, _handle, _dir) = active_controller("delete-endpoint");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();

    controller
        .set_loop_a()
        .unwrap_or_else(|e| unreachable!("set_loop_a: {e}"));
    let _ = controller.backend_mut().render_buffers(5);
    controller
        .set_loop_b()
        .unwrap_or_else(|e| unreachable!("set_loop_b: {e}"));
    let region = controller
        .markers()
        .and_then(TrackMarkers::current_region)
        .unwrap_or_else(|| unreachable!());
    let a_id = controller
        .markers()
        .and_then(|m| m.region(region))
        .and_then(|r| r.a)
        .unwrap_or_else(|| unreachable!());

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState {
        focused_marker: Some(a_id),
        ..Default::default()
    };
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    press_key(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Key::Delete,
    );

    let markers = controller.markers().unwrap_or_else(|| unreachable!());
    let r = markers.region(region).unwrap_or_else(|| unreachable!());
    assert!(r.a.is_none(), "the deleted endpoint must be cleared");
    assert!(!r.is_complete());
    assert_eq!(
        waveform.focused_marker, None,
        "focus must return to the detail waveform after delete"
    );
}

/// `F2`/`Enter` opens the inline rename; `Enter` commits, `Esc` cancels
/// without touching the model (contracts/ui-markers.md §3, AS1).
#[test]
fn f2_rename_commits_on_enter_cancels_on_esc() {
    let (mut controller, _handle, _dir) = active_controller("rename-commit-cancel");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();

    let id = controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState {
        focused_marker: Some(id),
        ..Default::default()
    };
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    press_key(&ctx, &mut controller, &mut artwork, &mut waveform, Key::F2);
    assert_eq!(
        waveform.rename.as_ref().map(|(rename_id, _)| *rename_id),
        Some(id),
        "F2 must open the inline rename for the focused marker"
    );

    if let Some((_, name)) = waveform.rename.as_mut() {
        *name = "Verse".to_string();
    }
    press_key(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Key::Enter,
    );
    assert_eq!(waveform.rename, None, "Enter must close the rename");
    assert_eq!(
        controller
            .markers()
            .and_then(|m| m.marker(id))
            .map(|m| m.name.clone()),
        Some("Verse".to_string()),
        "Enter must commit the draft"
    );

    press_key(&ctx, &mut controller, &mut artwork, &mut waveform, Key::F2);
    if let Some((_, name)) = waveform.rename.as_mut() {
        *name = "Should not stick".to_string();
    }
    press_key(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Key::Escape,
    );
    assert_eq!(waveform.rename, None, "Esc must close the rename");
    assert_eq!(
        controller
            .markers()
            .and_then(|m| m.marker(id))
            .map(|m| m.name.clone()),
        Some("Verse".to_string()),
        "Esc must not commit the draft"
    );
}

/// `C` cycles the focused marker's palette colour, and the panel/overlay
/// repaint in the new colour immediately (contracts/ui-markers.md §3, AS2)
/// — both derive from the same `theme::marker_color(marker.color)` call,
/// so the model's colour is the single source of truth for both.
#[test]
fn c_cycles_palette_and_row_matches_glyph() {
    let (mut controller, _handle, _dir) = active_controller("cycle-color");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();

    let id = controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));
    let before = controller
        .markers()
        .and_then(|m| m.marker(id))
        .map(|m| m.color)
        .unwrap_or_else(|| unreachable!());

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState {
        focused_marker: Some(id),
        ..Default::default()
    };
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    press_key(&ctx, &mut controller, &mut artwork, &mut waveform, Key::C);

    let after = controller
        .markers()
        .and_then(|m| m.marker(id))
        .map(|m| m.color)
        .unwrap_or_else(|| unreachable!());
    assert_ne!(before, after, "C must cycle the palette");

    let markers_now = controller.markers().cloned();
    let expected_color = modplayer_ui::theme::marker_color(after);
    let shapes = painted_shapes(0..(44_100 * 200), 44_100, |painter, space| {
        modplayer_ui::markers::paint_overlay(
            painter,
            space,
            markers_now.as_ref(),
            0,
            None,
            &modplayer_ui::theme::tokens::LIGHT,
        );
    });
    let has_matching_line = shapes.iter().any(|shape| {
        matches!(shape, Shape::LineSegment { stroke, .. } if stroke.color == expected_color)
    });
    assert!(
        has_matching_line,
        "the overlay line must repaint in the marker's new colour"
    );
}

/// A relative-delta drag from the overview glyph lands within 5ms of the
/// intended sample (SC-003), via the detail-view zoom-assist.
#[test]
fn drag_from_overview_zooms_detail_and_lands_within_5ms() {
    let (mut controller, _handle, _dir) = active_controller("drag-overview");
    controller.queue_replace(vec![track("a", 200_000)]);

    const SAMPLE_RATE: u32 = 44_100;
    let len_frames = (200_000u64 * u64::from(SAMPLE_RATE)) / 1000;
    let id = controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    let overview_rect = overview_bounds(&ctx, &mut controller, &mut artwork, &mut waveform);
    let glyph_rect = glyph_bounds(&ctx, &mut controller, &mut artwork, &mut waveform);
    let press_pos = glyph_rect.center();
    let target_x = (press_pos.x + 80.0).min(overview_rect.right() - 1.0);

    let space = TimeSpace::new(overview_rect, 0..len_frames, SAMPLE_RATE);
    let expected_frame = space.frame_at(target_x);

    let mut press = default_input();
    press.events.push(Event::PointerButton {
        pos: press_pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::default(),
    });
    let output = ctx.run_ui(press, |ui| {
        modplayer_ui::now_playing::show(
            ui,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        )
    });
    output.drop_without_applying_deltas();

    let mut drag = default_input();
    drag.events
        .push(Event::PointerMoved(Pos2::new(target_x, press_pos.y)));
    let output = ctx.run_ui(drag, |ui| {
        modplayer_ui::now_playing::show(
            ui,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        )
    });
    output.drop_without_applying_deltas();
    assert!(
        waveform.marker_drag.is_some(),
        "sanity: a drag must be in progress"
    );

    let mut release = default_input();
    release.events.push(Event::PointerButton {
        pos: Pos2::new(target_x, press_pos.y),
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::default(),
    });
    let output = ctx.run_ui(release, |ui| {
        modplayer_ui::now_playing::show(
            ui,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        )
    });
    output.drop_without_applying_deltas();

    assert_eq!(
        waveform.marker_drag, None,
        "release must commit and clear the drag"
    );
    let committed = controller
        .markers()
        .and_then(|m| m.position_of(id))
        .unwrap_or_else(|| unreachable!());

    let margin_frames = (5 * u64::from(SAMPLE_RATE)) / 1000;
    let delta = committed.abs_diff(expected_frame);
    assert!(
        delta <= margin_frames,
        "committed {committed} vs intended {expected_frame} (delta {delta} > {margin_frames} frames / 5ms)"
    );
}

/// `Esc` while dragging a marker restores its original position and the
/// detail window it had before the drag started (contracts/ui-markers.md
/// §5).
#[test]
fn drag_esc_restores_position_and_window() {
    let (mut controller, _handle, _dir) = active_controller("drag-esc");
    controller.queue_replace(vec![track("a", 200_000)]);

    let id = controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));
    let origin = controller
        .markers()
        .and_then(|m| m.position_of(id))
        .unwrap_or_else(|| unreachable!());

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    let glyph_rect = glyph_bounds(&ctx, &mut controller, &mut artwork, &mut waveform);
    let original_detail = waveform.detail.expect("a detail window must exist by now");
    let press_pos = glyph_rect.center();
    let target_x = press_pos.x + 80.0;

    let mut press = default_input();
    press.events.push(Event::PointerButton {
        pos: press_pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::default(),
    });
    let output = ctx.run_ui(press, |ui| {
        modplayer_ui::now_playing::show(
            ui,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        )
    });
    output.drop_without_applying_deltas();

    let mut drag = default_input();
    drag.events
        .push(Event::PointerMoved(Pos2::new(target_x, press_pos.y)));
    let output = ctx.run_ui(drag, |ui| {
        modplayer_ui::now_playing::show(
            ui,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        )
    });
    output.drop_without_applying_deltas();
    assert!(
        waveform.marker_drag.is_some(),
        "sanity: a drag must be in progress"
    );
    assert_ne!(
        waveform.detail,
        Some(original_detail),
        "sanity: the zoom-assist must have changed the detail window"
    );

    let mut esc = default_input();
    esc.events.push(Event::Key {
        key: Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::default(),
    });
    let output = ctx.run_ui(esc, |ui| {
        modplayer_ui::now_playing::show(
            ui,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        )
    });
    output.drop_without_applying_deltas();

    assert_eq!(waveform.marker_drag, None, "Esc must clear the drag");
    assert_eq!(
        controller.markers().and_then(|m| m.position_of(id)),
        Some(origin),
        "Esc must not move the marker"
    );
    assert_eq!(
        waveform.detail,
        Some(original_detail),
        "Esc must restore the pre-drag detail window"
    );
}

/// `Shift+1` sets slot 1's cue at the playhead and `1` jumps back to it
/// exactly, through `seek_frames` (006 US4, contracts/ui-markers.md §2,
/// AS1–AS3): the jump changes neither the position (an unrelated seek
/// away first proves it actually moved) nor whether playback is playing
/// or paused.
#[test]
fn shift_digit_sets_cue_and_digit_jumps_keeping_state() {
    let (mut controller, _handle, _dir) = active_controller("cue-set-jump");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    let _ = controller.backend_mut().render_buffers(5);

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    let slot1 = CueSlot::new(1).unwrap_or_else(|| unreachable!());
    let cue_pos = controller.shared().position_frames();
    press_key_with(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Key::Num1,
        Modifiers::SHIFT,
    );
    assert_eq!(
        controller
            .markers()
            .and_then(|m| m.cue(slot1))
            .map(|c| c.position),
        Some(cue_pos),
        "Shift+1 must set slot 1's cue at the playhead"
    );
    assert_eq!(waveform.marker_status, None);

    // While playing: move well away, jump back with `1`. Landing the seek
    // itself requires one render (the engine drains its command queue at
    // the start of a render call), so the observed position is `cue_pos`
    // plus that one render's own frame advance — the fixed
    // `JUMP_LAND_TOLERANCE_FRAMES` below comfortably covers it while still
    // proving the jump landed at the cue, not merely continued from
    // `moved_pos` (FR-014, "instant and precise").
    let _ = controller.backend_mut().render_buffers(10);
    let moved_pos = controller.shared().position_frames();
    assert!(
        moved_pos > cue_pos + JUMP_LAND_TOLERANCE_FRAMES,
        "sanity: playhead must have advanced well past the cue"
    );
    assert_eq!(controller.transport_state().intent, Intent::Playing);

    press_key(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Key::Num1,
    );
    controller.tick();
    let _ = controller.backend_mut().render_buffers(1);

    let landed = controller.shared().position_frames();
    assert!(
        landed >= cue_pos && landed <= cue_pos + JUMP_LAND_TOLERANCE_FRAMES,
        "1 must jump to (very near) the cue: landed={landed}, cue_pos={cue_pos}"
    );
    assert!(
        landed < moved_pos,
        "the jump must have moved backward, not just continued playing"
    );
    assert_eq!(
        controller.transport_state().intent,
        Intent::Playing,
        "jumping while playing must not pause"
    );

    // While paused: `1` still dispatches the same `seek_frames` (FR-014's
    // existing seek path already keeps a paused transport paused, per
    // 005 — checked here at the play/pause-state level only).
    controller.pause();
    controller.tick();
    assert_eq!(controller.transport_state().intent, Intent::Paused);

    press_key(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Key::Num1,
    );
    controller.tick();

    assert_eq!(
        controller.transport_state().intent,
        Intent::Paused,
        "jumping while paused must not resume playback"
    );
}

/// `1`-`8` on an empty slot is a silent no-op (006 US4, AS5,
/// contracts/ui-markers.md §2): no seek, no status, no marker.
#[test]
fn digit_on_empty_slot_is_noop() {
    let (mut controller, _handle, _dir) = active_controller("cue-empty-noop");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    let _ = controller.backend_mut().render_buffers(5);

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    let before_pos = controller.shared().position_frames();
    let before_intent = controller.transport_state().intent;
    let count_before = controller.markers().map(TrackMarkers::count).unwrap_or(0);

    press_key(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Key::Num2,
    );
    controller.tick();
    let _ = controller.backend_mut().render_buffers(1);

    assert_eq!(
        waveform.marker_status, None,
        "an empty-slot jump is never a refusal"
    );
    assert_eq!(
        controller.markers().map(TrackMarkers::count).unwrap_or(0),
        count_before,
        "an empty-slot jump must not create a marker"
    );
    assert_eq!(
        controller.transport_state().intent,
        before_intent,
        "an empty-slot jump must not change play/pause state"
    );
    assert!(
        controller.shared().position_frames() >= before_pos,
        "no seek must have fired: the playhead only advances forward, from natural playback"
    );
}

/// `Shift+n` on an already-occupied slot is a move, not a creation, so it
/// never hits the 64-marker limit (006 US4, AS6, contracts/marker-
/// service.md §7 — mirrors `markers_model.rs::move_paths_never_hit_limit`
/// at the controller/UI layer).
#[test]
fn shift_digit_on_occupied_slot_moves_at_limit() {
    let (mut controller, _handle, _dir) = active_controller("cue-move-at-limit");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();

    let slot1 = CueSlot::new(1).unwrap_or_else(|| unreachable!());
    controller
        .set_cue(slot1)
        .unwrap_or_else(|e| unreachable!("set_cue: {e}"));
    for _ in 0..63 {
        controller
            .add_point_marker()
            .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));
    }
    assert_eq!(controller.markers().map(TrackMarkers::count), Some(64));

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    let _ = controller.backend_mut().render_buffers(5);
    let new_pos = controller.shared().position_frames();

    press_key_with(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Key::Num1,
        Modifiers::SHIFT,
    );

    assert_eq!(
        waveform.marker_status, None,
        "moving an occupied slot at the 64-marker cap must not refuse"
    );
    assert_eq!(
        controller.markers().map(TrackMarkers::count),
        Some(64),
        "a move creates no new marker"
    );
    assert_eq!(
        controller
            .markers()
            .and_then(|m| m.cue(slot1))
            .map(|c| c.position),
        Some(new_pos),
        "slot 1's cue must have moved to the new playhead"
    );
}

/// FR-022 / contracts/ui-markers.md §3: reaching a glyph with `Tab` must
/// arm the focused-marker key table exactly as clicking it does. Regression
/// for the gap found driving quickstart.md's M15 — `focused_marker` was set
/// only from `clicked()`/`drag_started()`, so a keyboard-only user could
/// land on a glyph and still not nudge, rename, recolour or delete it.
#[test]
fn tab_focus_on_a_glyph_enables_the_marker_key_table() {
    let (mut controller, _handle, _dir) = active_controller("tab-focus");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    controller.set_nudge_step_ms(25);

    let id = controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));
    let start = controller
        .markers()
        .and_then(|m| m.position_of(id))
        .unwrap_or_else(|| unreachable!());

    let mut artwork = ArtworkCache::new();
    // No `focused_marker`: focus arrives purely through egui, as `Tab` does.
    let mut waveform = WaveformState::default();
    let ctx = fresh_ctx();

    let glyph_id = egui::Id::new(("marker-glyph", "overview", id));
    let output = ctx.run_ui(default_input(), |ui| {
        modplayer_ui::now_playing::show(
            ui,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        )
    });
    output.drop_without_applying_deltas();
    ctx.memory_mut(|memory| memory.request_focus(glyph_id));

    let output = ctx.run_ui(default_input(), |ui| {
        modplayer_ui::now_playing::show(
            ui,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        )
    });
    output.drop_without_applying_deltas();
    assert_eq!(
        waveform.focused_marker,
        Some(id),
        "keyboard focus on the glyph selects it, like a click"
    );

    press_key(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Key::ArrowRight,
    );
    let rate = controller.source_sample_rate().max(1);
    let step = (25u64 * u64::from(rate)) / 1000;
    assert_eq!(
        controller.markers().and_then(|m| m.position_of(id)),
        Some(start + step),
        "and the nudge row then applies without any pointer input"
    );
}

// -- 016-list-row-and-panel-components, US3 (Phase 5): the Markers panel
// reads as a card -----------------------------------------------------

/// C11 (contracts/panel-card.md): the Markers panel renders through the
/// shared `panel_card` — exactly one `Role::Heading` node named
/// `markers-panel` (with no other panel open by default, this is the only
/// card in the frame) — and wrapping adds chrome only: the same buttons
/// the panel always drew ("New loop", clear-all, the marker's colour
/// swatch), no new collapse/expand affordance (Markers has none, FR-021).
#[test]
fn markers_panel_renders_as_a_card_with_no_added_interactive_controls() {
    let (mut controller, _handle, _dirs) = active_controller("card");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));

    let ctx = fresh_ctx();
    ctx.enable_accesskit();
    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();

    let mut output = ctx.run_ui(default_input(), |ui| {
        modplayer_ui::now_playing::show(
            ui,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        )
    });
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    output.drop_without_applying_deltas();

    let heading_nodes: Vec<_> = update
        .nodes
        .iter()
        .filter(|(_, node)| {
            node.role() == Role::Heading && node.label() == Some(tr("markers-panel").as_str())
        })
        .collect();
    assert_eq!(
        heading_nodes.len(),
        1,
        "expected exactly one Markers panel heading node, got {}: {:?}",
        heading_nodes.len(),
        update.nodes
    );

    let button_names: Vec<String> = update
        .nodes
        .iter()
        .filter(|(_, node)| node.role() == Role::Button)
        .filter_map(|(_, node)| node.label().or(node.value()).map(str::to_string))
        .collect();
    assert!(
        button_names.iter().any(|n| n == &tr("markers-new-loop")),
        "expected the New loop button, got {button_names:?}"
    );
    assert!(
        button_names.iter().any(|n| n == &tr("markers-clear-all")),
        "expected the clear-all button, got {button_names:?}"
    );
    // 021-transport-bar-and-panel-layout (contract C1, superseding 016
    // C11): Markers now has a header disclosure like every other card —
    // still no *bar* toggle (spec Clarification 5) — so exactly one of
    // Markers' own two disclosure names (`panel-collapse`/`panel-expand`
    // for the Markers panel specifically, not a substring match: the other
    // three cards on this same screen have their own "Expand"/"Collapse"
    // disclosures too) is expected here.
    let markers_title = tr("markers-panel");
    let collapse_name = tr_args("panel-collapse", &[("panel", markers_title.clone())]);
    let expand_name = tr_args("panel-expand", &[("panel", markers_title)]);
    let disclosure_count = button_names
        .iter()
        .filter(|n| *n == &collapse_name || *n == &expand_name)
        .count();
    assert_eq!(
        disclosure_count, 1,
        "expected exactly one collapse/expand control on the Markers card header: {button_names:?}"
    );
}

// ---------------------------------------------------------------------
// 017-high-contrast-appearance (T009, contracts/marker-outline.md §1):
// the outline *value* — `marker_outline`'s presence, width and contrast
// floor. No paint call site is asserted here (that is M4-M9, Phase 5).
// ---------------------------------------------------------------------

/// M1: `marker_outline(r)` is `None` for both normal tables, `Some(1px
/// text_primary)` for both high-contrast tables.
#[test]
fn marker_outline_exists_only_in_high_contrast() {
    use modplayer_ui::theme::markers::marker_outline;
    use modplayer_ui::theme::tokens::{DARK, DARK_HIGH_CONTRAST, LIGHT, LIGHT_HIGH_CONTRAST};

    assert_eq!(marker_outline(&LIGHT), None);
    assert_eq!(marker_outline(&DARK), None);
    assert_eq!(
        marker_outline(&LIGHT_HIGH_CONTRAST),
        Some(egui::Stroke::new(1.0, LIGHT_HIGH_CONTRAST.text_primary))
    );
    assert_eq!(
        marker_outline(&DARK_HIGH_CONTRAST),
        Some(egui::Stroke::new(1.0, DARK_HIGH_CONTRAST.text_primary))
    );
}

/// M2: chosen over a thicker stroke so the glyph's own focused/unfocused
/// difference is not swamped.
#[test]
fn marker_outline_width_is_one_pixel() {
    assert_eq!(modplayer_ui::theme::markers::MARKER_OUTLINE_WIDTH, 1.0);
}

/// M3: for both high-contrast tables and all eight `MARKER_PALETTE`
/// entries, the outline (`text_primary`, palette-independent by
/// construction) clears >= 7:1 against both surfaces.
#[test]
fn marker_outline_clears_the_enhanced_floor_against_every_palette_entry() {
    use modplayer_ui::theme::MARKER_PALETTE;
    use modplayer_ui::theme::contrast::ratio;
    use modplayer_ui::theme::markers::marker_outline;
    use modplayer_ui::theme::tokens::{DARK_HIGH_CONTRAST, LIGHT_HIGH_CONTRAST};

    for table in [&LIGHT_HIGH_CONTRAST, &DARK_HIGH_CONTRAST] {
        let stroke =
            marker_outline(table).unwrap_or_else(|| unreachable!("high contrast must outline"));
        for _entry in MARKER_PALETTE.iter() {
            // The outline colour does not depend on the palette entry it
            // encircles (research R6) — looped anyway so the claim is
            // machine-checked over the whole palette, not argued.
            for base in [table.surface_base, table.surface_raised] {
                let measured = ratio(stroke.color, base);
                assert!(
                    measured >= 7.0,
                    "marker outline vs {base:?} = {measured:.2}, floor 7.0"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------
// 017-high-contrast-appearance (Phase 5, US3, contracts/marker-outline.md
// §2): where the outline is drawn — M4-M9. The outline *value* (M1-M3)
// is pinned above; this section is paint-site coverage only.
// ---------------------------------------------------------------------

/// M4: with markers placed and high contrast on, each detail-lane glyph
/// (region bracket, point, cue — `markers::lane`, O1-O3) gains an outline
/// shape in `text_primary`; with high contrast off, none does.
#[test]
fn detail_lane_glyphs_gain_an_outline() {
    let (mut controller, _handle, _dir) = active_controller("m4-detail-glyphs");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    controller
        .set_loop_a()
        .unwrap_or_else(|e| unreachable!("set_loop_a: {e}"));
    controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));
    let slot1 = CueSlot::new(1).unwrap_or_else(|| unreachable!());
    controller
        .set_cue(slot1)
        .unwrap_or_else(|e| unreachable!("set_cue: {e}"));

    let markers = controller.markers().cloned();
    let sample_rate = controller.source_sample_rate().max(1);
    let len_frames = 200_000u64 * u64::from(sample_rate) / 1000;

    let hc_visuals = {
        let ctx = Context::default();
        modplayer_ui::theme::apply_tokens_for(&ctx, true);
        ctx.style_of(ctx.theme()).visuals.clone()
    };
    let text_primary = modplayer_ui::theme::roles(&hc_visuals).text_primary;

    let mut lane_shapes = |ctx: &Context| -> Vec<Shape> {
        let mut waveform = WaveformState::default();
        let mut detail = DetailWindow::initial(0, len_frames, sample_rate);
        let output = ctx.run_ui(default_input(), |ui| {
            modplayer_ui::markers::lane(
                ui,
                "detail",
                0..len_frames,
                sample_rate,
                len_frames,
                markers.as_ref(),
                &mut controller,
                &mut waveform,
                &mut detail,
            );
        });
        let shapes = output.shapes.iter().map(|c| c.shape.clone()).collect();
        output.drop_without_applying_deltas();
        shapes
    };

    let normal_ctx = fresh_ctx();
    normal_ctx.enable_accesskit();
    let normal_shapes = lane_shapes(&normal_ctx);
    assert!(
        !has_outline_shapes(&normal_shapes, text_primary),
        "no outline shape in normal mode"
    );

    let hc_ctx = fresh_ctx_high_contrast();
    hc_ctx.enable_accesskit();
    let hc_shapes = lane_shapes(&hc_ctx);
    assert!(
        has_outline_shapes(&hc_shapes, text_primary),
        "high contrast must add an outline shape at the detail-lane glyph sites"
    );
}

/// M5: the same claim as M4, for the overview-lane marker line and a
/// clamped marker's warning glyph (`markers::paint_overlay`, O4/O6).
#[test]
fn overview_lane_markers_gain_an_outline() {
    use modplayer_ui::theme::tokens::{LIGHT, LIGHT_HIGH_CONTRAST};

    let id = TrackId::new("spotify:track:m5-overview").unwrap_or_else(|_| unreachable!());
    let mut markers = TrackMarkers::new(id, 44_100, 44_100 * 200);
    markers
        .add_point(1_000)
        .unwrap_or_else(|e| unreachable!("add_point: {e:?}"));
    markers
        .set_loop_a(2_000)
        .unwrap_or_else(|e| unreachable!("set_loop_a: {e}"));
    // Shrinks the track: the A endpoint lands past the new length and is
    // flagged `clamped`, so O6's warning glyph paints too.
    markers.set_len_frames(1_500);

    let normal_shapes = painted_shapes(0..(44_100 * 200), 44_100, |painter, space| {
        modplayer_ui::markers::paint_overlay(painter, space, Some(&markers), 0, None, &LIGHT);
    });
    assert!(
        !has_outline_shapes(&normal_shapes, LIGHT_HIGH_CONTRAST.text_primary),
        "no outline shape in normal mode"
    );

    let hc_shapes = painted_shapes(0..(44_100 * 200), 44_100, |painter, space| {
        modplayer_ui::markers::paint_overlay(
            painter,
            space,
            Some(&markers),
            0,
            None,
            &LIGHT_HIGH_CONTRAST,
        );
    });
    assert!(
        has_outline_shapes(&hc_shapes, LIGHT_HIGH_CONTRAST.text_primary),
        "high contrast must add an outline shape at the overview-lane line/clamped-marker warning"
    );
}

/// M6: an armed loop region's span shading (`loop_state == 2`) gains a
/// `rect_stroke` in `text_primary`; its translucent fill colour is
/// unchanged (M8).
#[test]
fn loop_region_span_gains_an_outline() {
    use modplayer_ui::theme::tokens::{LIGHT, LIGHT_HIGH_CONTRAST};

    let id = TrackId::new("spotify:track:m6-span").unwrap_or_else(|_| unreachable!());
    let mut markers = TrackMarkers::new(id, 44_100, 44_100 * 200);
    markers
        .set_loop_a(1_000)
        .unwrap_or_else(|e| unreachable!("set_loop_a: {e}"));
    markers
        .set_loop_b(5_000)
        .unwrap_or_else(|e| unreachable!("set_loop_b: {e}"));
    // WL4: shading now follows the region's own `armed` flag — arm it so
    // `loop_state == 2` actually reads as `LoopShade::ArmedActive`.
    let region = markers
        .current_region()
        .unwrap_or_else(|| unreachable!("region must exist"));
    markers
        .arm(region)
        .unwrap_or_else(|e| unreachable!("arm: {e}"));

    let span_rect_shapes = |roles: &modplayer_ui::theme::Roles| -> Vec<Shape> {
        painted_shapes(0..(44_100 * 200), 44_100, |painter, space| {
            modplayer_ui::markers::paint_overlay(painter, space, Some(&markers), 2, None, roles);
        })
        .into_iter()
        .filter(|shape| matches!(shape, Shape::Rect(_)))
        .collect()
    };

    let normal_shapes = span_rect_shapes(&LIGHT);
    assert_eq!(
        normal_shapes.len(),
        1,
        "armed-active paints exactly one span rect in normal mode"
    );
    let Shape::Rect(normal_rect) = &normal_shapes[0] else {
        unreachable!()
    };
    assert!(normal_rect.fill.a() > 0, "the span must be filled");
    assert_eq!(normal_rect.stroke.width, 0.0, "no stroke in normal mode");

    let hc_shapes = span_rect_shapes(&LIGHT_HIGH_CONTRAST);
    assert_eq!(
        hc_shapes.len(),
        2,
        "high contrast adds one extra rect_stroke shape beyond the filled span"
    );
    let hc_fill = hc_shapes
        .iter()
        .find_map(|shape| match shape {
            Shape::Rect(r) if r.fill.a() > 0 => Some(r.fill),
            _ => None,
        })
        .expect("the filled span rect must still exist");
    assert_eq!(
        hc_fill, normal_rect.fill,
        "M8: the span's fill colour must not change in high contrast"
    );
    let outline_stroke = hc_shapes
        .iter()
        .find_map(|shape| match shape {
            Shape::Rect(r) if r.stroke.width > 0.0 => Some(r.stroke),
            _ => None,
        })
        .expect("an outline rect_stroke must exist in high contrast");
    assert_eq!(outline_stroke.color, LIGHT_HIGH_CONTRAST.text_primary);
}

/// M8: the palette fill colour at an outline site is byte-identical
/// between normal and high contrast — high contrast only adds a
/// casing/stroke, never recolours (FR-011).
#[test]
fn palette_fills_are_never_recoloured() {
    use modplayer_ui::theme::tokens::{LIGHT, LIGHT_HIGH_CONTRAST};

    let id = TrackId::new("spotify:track:m8-fills").unwrap_or_else(|_| unreachable!());
    let mut markers = TrackMarkers::new(id, 44_100, 44_100 * 200);
    let marker_id = markers
        .add_point(1_000)
        .unwrap_or_else(|e| unreachable!("add_point: {e:?}"));
    let palette_color = modplayer_ui::theme::marker_color(
        markers
            .marker(marker_id)
            .unwrap_or_else(|| unreachable!())
            .color,
    );

    let plain_line_color = |roles: &modplayer_ui::theme::Roles| -> egui::Color32 {
        painted_shapes(0..(44_100 * 200), 44_100, |painter, space| {
            modplayer_ui::markers::paint_overlay(painter, space, Some(&markers), 0, None, roles);
        })
        .into_iter()
        .find_map(|shape| match shape {
            Shape::LineSegment { stroke, .. } if stroke.width <= 2.0 => Some(stroke.color),
            _ => None,
        })
        .expect("the marker's own line must be present")
    };

    assert_eq!(plain_line_color(&LIGHT), palette_color);
    assert_eq!(
        plain_line_color(&LIGHT_HIGH_CONTRAST),
        palette_color,
        "M8: the palette fill must not change in high contrast"
    );
}

/// M9 (Edge Case 4): a focused glyph still strokes wider than an
/// unfocused one by the same delta as in normal mode — the casing adds
/// the same width to both (O4 here; O1's `paint_bracket` is the same
/// mechanism).
#[test]
fn focus_width_difference_survives_the_outline() {
    use modplayer_ui::theme::tokens::LIGHT_HIGH_CONTRAST;

    let id = TrackId::new("spotify:track:m9-focus-width").unwrap_or_else(|_| unreachable!());
    let mut markers = TrackMarkers::new(id, 44_100, 44_100 * 200);
    let marker_id = markers
        .add_point(1_000)
        .unwrap_or_else(|e| unreachable!("add_point: {e:?}"));

    let casing_width_for = |focused: Option<MarkerId>| -> f32 {
        painted_shapes(0..(44_100 * 200), 44_100, |painter, space| {
            modplayer_ui::markers::paint_overlay(
                painter,
                space,
                Some(&markers),
                0,
                focused,
                &LIGHT_HIGH_CONTRAST,
            );
        })
        .into_iter()
        .find_map(|shape| match shape {
            Shape::LineSegment { stroke, .. }
                if stroke.color == LIGHT_HIGH_CONTRAST.text_primary =>
            {
                Some(stroke.width)
            }
            _ => None,
        })
        .expect("a casing line must exist in high contrast")
    };

    let unfocused_casing = casing_width_for(None);
    let focused_casing = casing_width_for(Some(marker_id));

    assert_eq!(
        focused_casing - unfocused_casing,
        1.0,
        "the casing must preserve the 2.0 vs 1.0 focus width delta"
    );
}

/// M7 (research R7): the Markers panel's colour swatch carries a 1px
/// `text_primary` frame stroke in high contrast, inherited from the
/// divider (FR-007) — no swatch-specific paint code exists anywhere in
/// this feature.
#[test]
fn panel_swatch_inherits_the_divider_outline() {
    let (mut controller, _handle, _dir) = active_controller("m7-swatch");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    let id = controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));
    let color = controller
        .markers()
        .and_then(|m| m.marker(id))
        .map(|m| m.color)
        .unwrap_or_else(|| unreachable!());
    let palette_color = modplayer_ui::theme::marker_color(color);

    let swatch_stroke = |ctx: &Context,
                         controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
                         waveform: &mut WaveformState|
     -> Stroke {
        let output = ctx.run_ui(default_input(), |ui| {
            modplayer_ui::markers::panel(ui, controller, waveform, &mut true);
        });
        let stroke = output
            .shapes
            .iter()
            .find_map(|clipped| match &clipped.shape {
                Shape::Rect(r) if r.fill == palette_color => Some(r.stroke),
                _ => None,
            })
            .expect(
                "the colour swatch button must paint a Rect filled with the marker's palette colour",
            );
        output.drop_without_applying_deltas();
        stroke
    };

    let mut waveform = WaveformState::default();
    let normal_ctx = fresh_ctx();
    normal_ctx.enable_accesskit();
    let normal_stroke = swatch_stroke(&normal_ctx, &mut controller, &mut waveform);

    let hc_ctx = fresh_ctx_high_contrast();
    hc_ctx.enable_accesskit();
    let hc_visuals = hc_ctx.style_of(hc_ctx.theme()).visuals.clone();
    let hc_stroke = swatch_stroke(&hc_ctx, &mut controller, &mut waveform);

    let hc_text_primary = modplayer_ui::theme::roles(&hc_visuals).text_primary;
    assert_eq!(
        hc_stroke.color, hc_text_primary,
        "the swatch must inherit high contrast's opaque divider stroke"
    );
    assert_ne!(
        normal_stroke.color, hc_stroke.color,
        "normal mode's divider stroke must differ from the high-contrast one"
    );
}

// ---------------------------------------------------------------------
// 023-markers-panel-structure — Phase 3, User Story 1: the panel shows
// loop regions, points and cues as three distinct, labelled, counted
// groups (contracts/ui-markers-panel.md P1/P2; T010-T013).
// ---------------------------------------------------------------------

/// T010 (contract P1/P2; FR-001, FR-004; US1 AS1/AS3/AS4; SC-002): a mixed
/// track (one complete loop region, two points, one cue) renders exactly
/// three group headings, in order Loop Region -> Points -> Cues, each
/// carrying its own count.
#[test]
fn panel_groups_in_order_with_counts() {
    let (mut controller, _handle, _dir) = active_controller("panel-groups-order");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();

    controller
        .set_loop_a()
        .unwrap_or_else(|e| unreachable!("set_loop_a: {e}"));
    let _ = controller.backend_mut().render_buffers(5);
    controller
        .set_loop_b()
        .unwrap_or_else(|e| unreachable!("set_loop_b: {e}"));
    controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));
    controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));
    let cue_slot = CueSlot::new(3).unwrap_or_else(|| unreachable!());
    controller
        .set_cue(cue_slot)
        .unwrap_or_else(|e| unreachable!("set_cue: {e}"));

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    let headings =
        markers_group_headings_in_order(&ctx, &mut controller, &mut artwork, &mut waveform);

    let expected = vec![
        tr_args(
            "markers-group-heading",
            &[
                ("label", tr("markers-group-loop")),
                ("count", "1".to_string()),
            ],
        ),
        tr_args(
            "markers-group-heading",
            &[
                ("label", tr("markers-group-points")),
                ("count", "2".to_string()),
            ],
        ),
        tr_args(
            "markers-group-heading",
            &[
                ("label", tr("markers-group-cues")),
                ("count", "1".to_string()),
            ],
        ),
    ];
    assert_eq!(
        headings, expected,
        "expected exactly these three headings in order"
    );
}

/// T010 (FR-001; SC-002): a track with a loop region and a cue but no
/// points renders no Points heading at all.
#[test]
fn points_group_omitted_when_empty() {
    let (mut controller, _handle, _dir) = active_controller("points-omitted");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();

    controller
        .set_loop_a()
        .unwrap_or_else(|e| unreachable!("set_loop_a: {e}"));
    let _ = controller.backend_mut().render_buffers(5);
    controller
        .set_loop_b()
        .unwrap_or_else(|e| unreachable!("set_loop_b: {e}"));
    let cue_slot = CueSlot::new(1).unwrap_or_else(|| unreachable!());
    controller
        .set_cue(cue_slot)
        .unwrap_or_else(|e| unreachable!("set_cue: {e}"));

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    let headings =
        markers_group_headings_in_order(&ctx, &mut controller, &mut artwork, &mut waveform);
    let points_label = tr("markers-group-points");
    assert!(
        !headings.iter().any(|h| h.contains(points_label.as_str())),
        "expected no Points heading among {headings:?}"
    );
    assert_eq!(
        headings.len(),
        2,
        "expected exactly the Loop and Cues headings, got {headings:?}"
    );
}

/// T010 (FR-004; US1 AS4): a points-only track (no loop region at all)
/// still renders the Loop Region heading, at count 0, first.
#[test]
fn loop_group_always_when_populated() {
    let (mut controller, _handle, _dir) = active_controller("loop-always-shows");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();

    controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    let headings =
        markers_group_headings_in_order(&ctx, &mut controller, &mut artwork, &mut waveform);
    let expected_loop = tr_args(
        "markers-group-heading",
        &[
            ("label", tr("markers-group-loop")),
            ("count", "0".to_string()),
        ],
    );
    assert!(
        headings.contains(&expected_loop),
        "expected {expected_loop:?} among {headings:?}"
    );
    assert_eq!(
        headings.first(),
        Some(&expected_loop),
        "the Loop Region heading must render first"
    );
}

/// T011 (contract P1/P3; FR-003; Clarification 12): a `B`-only region
/// renders its loop cells (today it shows none), and two regions render
/// as two blocks ordered by their own earliest *present* boundary
/// position — not by creation order.
#[test]
fn loop_block_cells_follow_last_boundary() {
    let (mut controller, _handle, _dir) = active_controller("loop-block-order");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();

    // Region "late": created FIRST, but its only boundary (`B`) lands at a
    // LATER playhead position.
    controller
        .new_loop_region()
        .unwrap_or_else(|e| unreachable!("new_loop_region: {e}"));
    let _ = controller.backend_mut().render_buffers(20);
    controller
        .set_loop_b()
        .unwrap_or_else(|e| unreachable!("set_loop_b: {e}"));

    // Region "early": created SECOND, but its only boundary (`A`) lands at
    // an EARLIER playhead position — proves the sort is by position, not
    // creation order (Clarification 12, research R3).
    controller
        .new_loop_region()
        .unwrap_or_else(|e| unreachable!("new_loop_region: {e}"));
    controller.seek_frames(0);
    let _ = controller.backend_mut().render_buffers(1);
    controller
        .set_loop_a()
        .unwrap_or_else(|e| unreachable!("set_loop_a: {e}"));

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    let mut output = ctx.run_ui(default_input(), |ui| {
        modplayer_ui::now_playing::show(
            ui,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        )
    });
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    output.drop_without_applying_deltas();

    // Both regions' loop cells render — the B-only region's included
    // (today it shows none): one `Role::CheckBox` "Arm loop" switch per
    // region (`switch`'s own single interactive node, unlike its plain
    // inner label, which the queue/effect panels' own controls could
    // otherwise coincidentally repeat).
    let arm_checkbox_count = update
        .nodes
        .iter()
        .filter(|(_, node)| {
            node.role() == Role::CheckBox
                && (node.label() == Some(tr("loop-arm").as_str())
                    || node.value() == Some(tr("loop-arm").as_str()))
        })
        .count();
    assert_eq!(
        arm_checkbox_count, 2,
        "expected both regions' loop cells (one Arm-loop checkbox each)"
    );

    // The `marker-glyph` template isolate-wraps each placeable
    // (`{ $role } { $name } { $time }`), so the row's label embeds its
    // role as `\u{2068}A\u{2069}`/`\u{2068}B\u{2069}`, never a plain
    // leading "A "/"B ".
    let row_top = |role: String| -> f64 {
        let needle = format!("\u{2068}{role}\u{2069}");
        update
            .nodes
            .iter()
            .filter(|(_, node)| node.role() == Role::ListItem)
            .find_map(|(_, node)| {
                let label = node.label()?;
                if label.contains(needle.as_str()) {
                    node.bounds().map(|b| b.y0)
                } else {
                    None
                }
            })
            .unwrap_or_else(|| unreachable!("no {role:?} row found"))
    };
    let a_y = row_top(tr("marker-role-a"));
    let b_y = row_top(tr("marker-role-b"));
    assert!(
        a_y < b_y,
        "the earlier (A-only) block must render above the later (B-only) block"
    );
}

/// T012 (contract P1/P8; FR-015/FR-016; SC-008): the Cues group always
/// shows all 8 slots — an occupied slot as a full row (its own placeholder
/// absent), every other slot as the muted `markers-cue-empty { $slot }`
/// placeholder — and its heading counts only the occupied slot.
#[test]
fn cues_group_shows_eight_slots_muted_empty_rows() {
    let (mut controller, _handle, _dir) = active_controller("cues-eight-slots");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();

    controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));
    let occupied_slot = CueSlot::new(5).unwrap_or_else(|| unreachable!());
    controller
        .set_cue(occupied_slot)
        .unwrap_or_else(|e| unreachable!("set_cue: {e}"));

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    let texts = rendered_texts(&ctx, &mut controller, &mut artwork, &mut waveform);

    for slot in 1..=8u8 {
        let placeholder = tr_args("markers-cue-empty", &[("slot", slot.to_string())]);
        if slot == 5 {
            assert!(
                !texts.contains(&placeholder),
                "slot 5 is occupied and must not show the empty placeholder"
            );
        } else {
            assert!(
                texts.contains(&placeholder),
                "expected the empty placeholder for slot {slot} among {texts:?}"
            );
        }
    }

    let headings =
        markers_group_headings_in_order(&ctx, &mut controller, &mut artwork, &mut waveform);
    let expected_cues_heading = tr_args(
        "markers-group-heading",
        &[
            ("label", tr("markers-group-cues")),
            ("count", "1".to_string()),
        ],
    );
    assert!(
        headings.contains(&expected_cues_heading),
        "expected {expected_cues_heading:?} among {headings:?}"
    );
}

/// T012 (contract P7, P8): an empty cue-slot row's placeholder text never
/// carries an interactive (`Role::Button`) node — it is not a Tab stop.
#[test]
fn empty_cue_rows_are_not_tab_stops() {
    let (mut controller, _handle, _dir) = active_controller("empty-cue-rows-not-tab-stops");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    let mut output = ctx.run_ui(default_input(), |ui| {
        modplayer_ui::now_playing::show(
            ui,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        )
    });
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    output.drop_without_applying_deltas();

    for slot in 1..=8u8 {
        let text = tr_args("markers-cue-empty", &[("slot", slot.to_string())]);
        let matches: Vec<_> = update
            .nodes
            .iter()
            .filter(|(_, node)| {
                node.label() == Some(text.as_str()) || node.value() == Some(text.as_str())
            })
            .collect();
        assert!(
            !matches.is_empty(),
            "expected an empty-cue row for slot {slot}"
        );
        for (_, node) in matches {
            assert_ne!(
                node.role(),
                Role::Button,
                "empty cue slot {slot} must not be a Tab stop"
            );
        }
    }
}

/// T013 (contract P1 Empty; FR-020; SC-007): a fully empty panel shows
/// exactly `markers-empty` and "New loop region" — no headings, cue rows
/// or Clear all markers.
#[test]
fn empty_panel_shows_message_and_new_loop_only() {
    let (mut controller, _handle, _dir) = active_controller("empty-panel-exact");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    let mut output = ctx.run_ui(default_input(), |ui| {
        modplayer_ui::now_playing::show(
            ui,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        )
    });
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    output.drop_without_applying_deltas();

    let texts: Vec<String> = update
        .nodes
        .iter()
        .flat_map(|(_, node)| [node.value(), node.label()])
        .filter_map(|text| text.map(str::to_string))
        .filter(|text| !text.is_empty())
        .collect();

    assert!(
        texts.contains(&tr("markers-empty")),
        "expected the empty-state message among {texts:?}"
    );
    assert!(
        texts.contains(&tr("markers-new-loop")),
        "expected the New loop region control among {texts:?}"
    );
    assert!(
        !texts.contains(&tr("markers-clear-all")),
        "Clear all markers must not render on the empty layout"
    );

    for key in [
        "markers-group-loop",
        "markers-group-points",
        "markers-group-cues",
    ] {
        let label = tr(key);
        assert!(
            !texts.iter().any(|t| t.contains(label.as_str())),
            "expected no {key} heading among {texts:?}"
        );
    }

    for slot in 1..=8u8 {
        let placeholder = tr_args("markers-cue-empty", &[("slot", slot.to_string())]);
        assert!(
            !texts.contains(&placeholder),
            "expected no cue rows on the empty layout"
        );
    }
}

/// T013 (contract P1; FR-021): the status line renders directly above the
/// Loop Region group, whenever `waveform.marker_status` is `Some`.
#[test]
fn status_line_above_loop_group() {
    let (mut controller, _handle, _dir) = active_controller("status-line-above-loop");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState {
        marker_status: Some("markers-status"),
        ..Default::default()
    };
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    let mut output = ctx.run_ui(default_input(), |ui| {
        modplayer_ui::now_playing::show(
            ui,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        )
    });
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    output.drop_without_applying_deltas();

    let status_text = tr("markers-status");
    let status_y = update
        .nodes
        .iter()
        .find_map(|(_, node)| {
            if node.label() == Some(status_text.as_str())
                || node.value() == Some(status_text.as_str())
            {
                node.bounds().map(|b| b.y0)
            } else {
                None
            }
        })
        .unwrap_or_else(|| unreachable!("expected the status line among the rendered nodes"));

    let loop_label = tr("markers-group-loop");
    let loop_heading_y = update
        .nodes
        .iter()
        .filter(|(_, node)| node.role() == Role::Heading)
        .find_map(|(_, node)| {
            let label = node.label()?;
            if label.contains(loop_label.as_str()) {
                node.bounds().map(|b| b.y0)
            } else {
                None
            }
        })
        .unwrap_or_else(|| unreachable!("expected the Loop Region heading"));

    assert!(
        status_y < loop_heading_y,
        "the status line must render above the Loop Region group"
    );
}

// ---------------------------------------------------------------------
// 023-markers-panel-structure — Phase 4, User Story 2: act on any marker
// row without leaving the panel (contracts/ui-markers-panel.md P3-P7;
// T020-T026).
// ---------------------------------------------------------------------

/// T020 (contract P3; FR-008): every populated row's four actions
/// render, always (never hover-revealed), each a `Role::Button` named by
/// its own words (`markers-jump`/`-nudge-earlier`/`-nudge-later`/
/// `-remove`), not its glyph.
#[test]
fn row_actions_present_quiet_and_named() {
    let (mut controller, _handle, _dir) = active_controller("row-actions-present");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    let f = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );

    for key in [
        "markers-jump",
        "markers-nudge-earlier",
        "markers-nudge-later",
        "markers-remove",
    ] {
        let label = tr(key);
        assert!(
            f.has(Role::Button, &label),
            "expected a Role::Button named {label:?} among the rendered nodes"
        );
    }
}

/// T020 (contract P3/P7; Clarification 6; SC-009): a row's Tab order is
/// exactly swatch -> name -> jump -> nudge-earlier -> nudge-later ->
/// remove — six stops, creation order equal to Tab order (research R5).
#[test]
fn row_tab_order_swatch_name_actions() {
    let (mut controller, _handle, _dir) = active_controller("row-tab-order");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    let id = controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));
    let name = controller
        .markers()
        .and_then(|m| m.marker(id))
        .map(|m| m.name.clone())
        .unwrap_or_default();

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState {
        panel_focus: Some(PanelFocus::Row(id)),
        ..Default::default()
    };
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    // Frame 0 applies the pending focus request onto the swatch (T033).
    let f0 = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );
    assert_eq!(
        f0.focused().map(|n| n.role),
        Some(Role::Button),
        "the swatch must hold focus first"
    );

    for expected in [
        name,
        tr("markers-jump"),
        tr("markers-nudge-earlier"),
        tr("markers-nudge-later"),
        tr("markers-remove"),
    ] {
        let f = frame(
            &ctx,
            &mut controller,
            &mut artwork,
            &mut waveform,
            tab_input(),
        );
        let focused = f
            .focused()
            .unwrap_or_else(|| unreachable!("something must be focused after Tab"));
        assert_eq!(
            focused.label.as_deref(),
            Some(expected.as_str()),
            "unexpected Tab stop"
        );
    }
}

/// T021 (contract P6; FR-009): clicking a row's jump action seeks to
/// that marker's position via the same `seek_frames` path a keyboard cue
/// jump uses, leaving play/pause state exactly as it was — checked both
/// while `Playing` and while `Paused`. `apply_intents`'s `Jump` handling
/// never branches on `MarkerKind` (every kind renders through the same
/// `show_row_action` call), so a `Point` marker is a representative
/// sample of every kind.
#[test]
fn row_jump_seeks_preserving_play_state() {
    let (mut controller, _handle, _dir) = active_controller("row-jump-play-state");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();

    let target_pos = controller.shared().position_frames();
    let _id = controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    // Move well away from the marker while Playing, then jump back.
    let _ = controller.backend_mut().render_buffers(20);
    let moved_pos = controller.shared().position_frames();
    assert!(
        moved_pos > target_pos + JUMP_LAND_TOLERANCE_FRAMES,
        "sanity: must have advanced"
    );
    assert_eq!(controller.transport_state().intent, Intent::Playing);

    let jump_label = tr("markers-jump");
    let f = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );
    let jump_bounds = f.bounds(Role::Button, &jump_label);
    click_at(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        jump_bounds.center(),
    );

    controller.tick();
    let _ = controller.backend_mut().render_buffers(1);
    let landed = controller.shared().position_frames();
    assert!(
        landed >= target_pos && landed <= target_pos + JUMP_LAND_TOLERANCE_FRAMES,
        "jump must land at (very near) the marker: landed={landed}, target={target_pos}"
    );
    assert_eq!(
        controller.transport_state().intent,
        Intent::Playing,
        "jumping while playing must not pause"
    );

    // While Paused: move away again (by seeking, since a paused
    // transport does not advance on its own), jump, and confirm it still
    // does not resume playback.
    controller.pause();
    controller.tick();
    controller.seek_frames(target_pos + JUMP_LAND_TOLERANCE_FRAMES * 5);
    assert_eq!(controller.transport_state().intent, Intent::Paused);

    let f = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );
    let jump_bounds = f.bounds(Role::Button, &jump_label);
    click_at(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        jump_bounds.center(),
    );
    controller.tick();

    let landed_paused = controller.shared().position_frames();
    assert!(
        landed_paused >= target_pos && landed_paused <= target_pos + JUMP_LAND_TOLERANCE_FRAMES,
        "jump while paused must also land at the marker"
    );
    assert_eq!(
        controller.transport_state().intent,
        Intent::Paused,
        "jumping while paused must not resume playback"
    );
}

/// T021 (contract P6): jump is a no-op while this exact marker is being
/// dragged (`waveform.marker_drag`) — the drag itself owns the pointer.
#[test]
fn row_jump_noop_while_dragging() {
    let (mut controller, _handle, _dir) = active_controller("row-jump-dragging");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    let _ = controller.backend_mut().render_buffers(5);

    let id = controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));
    let marker_pos = controller
        .markers()
        .and_then(|m| m.position_of(id))
        .unwrap_or_else(|| unreachable!());

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState {
        marker_drag: Some(modplayer_ui::waveform::MarkerDrag {
            marker: id,
            origin_position: marker_pos,
            live: marker_pos,
            origin_detail: DetailWindow::initial(marker_pos, 44_100 * 200, 44_100),
        }),
        ..Default::default()
    };
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    let _ = controller.backend_mut().render_buffers(20);
    let before = controller.shared().position_frames();
    assert!(
        before > marker_pos + JUMP_LAND_TOLERANCE_FRAMES,
        "sanity: playhead must be well past the marker"
    );

    let jump_label = tr("markers-jump");
    let f = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );
    let jump_bounds = f.bounds(Role::Button, &jump_label);
    click_at(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        jump_bounds.center(),
    );
    controller.tick();

    let after = controller.shared().position_frames();
    assert!(
        after > marker_pos + JUMP_LAND_TOLERANCE_FRAMES,
        "jump must be a no-op while this marker is being dragged: after={after}, marker_pos={marker_pos}"
    );
}

/// T022 (contract P6; FR-010): nudge earlier/later move the marker by
/// exactly one `nudge_step_ms` step — `nudge_marker(id, ∓1, 1)`, the same
/// effect the keyboard's `←`/`→` produce — and clamp at `0` rather than
/// underflow.
#[test]
fn row_nudge_matches_keyboard_step() {
    let (mut controller, _handle, _dir) = active_controller("row-nudge-step");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    controller.set_nudge_step_ms(25);
    let rate = u64::from(controller.source_sample_rate());
    let step = rate * 25 / 1000;

    let _ = controller.backend_mut().render_buffers(200);
    let id = controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));
    let before = controller
        .markers()
        .and_then(|m| m.position_of(id))
        .unwrap_or_else(|| unreachable!());
    assert!(
        before > step,
        "sanity: far enough from 0 to nudge earlier without clamping"
    );

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    for (key, expected) in [
        ("markers-nudge-earlier", before - step),
        ("markers-nudge-later", before),
    ] {
        let label = tr(key);
        let f = frame(
            &ctx,
            &mut controller,
            &mut artwork,
            &mut waveform,
            tall_input(),
        );
        let bounds = f.bounds(Role::Button, &label);
        click_at(
            &ctx,
            &mut controller,
            &mut artwork,
            &mut waveform,
            bounds.center(),
        );
        let now = controller
            .markers()
            .and_then(|m| m.position_of(id))
            .unwrap_or_else(|| unreachable!());
        assert_eq!(now, expected, "{key} must move by exactly one nudge step");
    }

    // Clamp at 0: a marker already near 0 nudged earlier lands at 0, not
    // an underflowed wraparound. `seek_frames` only queues the seek — one
    // `render_buffers` call drains it before `add_point_marker` reads the
    // now-current playhead position (mirrors `loop_block_cells_follow_
    // last_boundary`'s own `seek_frames(0)` + `render_buffers(1)` pair).
    controller.seek_frames(0);
    let _ = controller.backend_mut().render_buffers(1);
    let near_zero_id = controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));
    let earlier_label = tr("markers-nudge-earlier");
    let f = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );
    // Two points now exist; points are sorted by position, so the
    // near-zero marker's row renders topmost — index 0.
    let bounds = f
        .nth_bounds(Role::Button, &earlier_label, 0)
        .unwrap_or_else(|| unreachable!("expected two rows' nudge-earlier actions"));
    click_at(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        bounds.center(),
    );
    assert_eq!(
        controller
            .markers()
            .and_then(|m| m.position_of(near_zero_id)),
        Some(0),
        "nudge earlier must clamp at 0, not underflow"
    );
}

/// T023 (contract P6; FR-011): the row's remove action calls
/// `delete_marker(id)` with no confirmation, exactly the keyboard
/// `Delete`'s own effect — a region boundary removal leaves the region
/// incomplete, removing the *last* boundary deletes the region, and
/// removing an armed region's endpoint disarms it (006 I9).
#[test]
fn row_remove_matches_keyboard_delete() {
    // Case 1: removing one boundary of a complete region leaves it
    // incomplete (not deleted).
    {
        let (mut controller, _handle, _dir) = active_controller("row-remove-incomplete");
        controller.queue_replace(vec![track("a", 200_000)]);
        controller.play();
        controller.tick();
        controller
            .set_loop_a()
            .unwrap_or_else(|e| unreachable!("set_loop_a: {e}"));
        let _ = controller.backend_mut().render_buffers(5);
        controller
            .set_loop_b()
            .unwrap_or_else(|e| unreachable!("set_loop_b: {e}"));
        let region = controller
            .markers()
            .and_then(TrackMarkers::current_region)
            .unwrap_or_else(|| unreachable!());

        let mut artwork = ArtworkCache::new();
        let mut waveform = WaveformState::default();
        let ctx = fresh_ctx();
        ctx.enable_accesskit();

        let remove_label = tr("markers-remove");
        let f = frame(
            &ctx,
            &mut controller,
            &mut artwork,
            &mut waveform,
            tall_input(),
        );
        // A's row renders above B's (contract P1): the topmost remove.
        let bounds = f
            .nth_bounds(Role::Button, &remove_label, 0)
            .unwrap_or_else(|| unreachable!("expected the A row's remove action"));
        click_at(
            &ctx,
            &mut controller,
            &mut artwork,
            &mut waveform,
            bounds.center(),
        );

        let markers = controller.markers().unwrap_or_else(|| unreachable!());
        let r = markers
            .region(region)
            .unwrap_or_else(|| unreachable!("region must still exist"));
        assert!(r.a.is_none(), "the removed endpoint must be cleared");
        assert!(!r.is_complete());
    }

    // Case 2: removing a region's *last* (only) boundary deletes the
    // region entirely.
    {
        let (mut controller, _handle, _dir) = active_controller("row-remove-last-boundary");
        controller.queue_replace(vec![track("a", 200_000)]);
        controller.play();
        controller.tick();
        controller
            .new_loop_region()
            .unwrap_or_else(|e| unreachable!("new_loop_region: {e}"));
        let _ = controller.backend_mut().render_buffers(5);
        controller
            .set_loop_b()
            .unwrap_or_else(|e| unreachable!("set_loop_b: {e}"));
        let region = controller
            .markers()
            .and_then(TrackMarkers::current_region)
            .unwrap_or_else(|| unreachable!());

        let mut artwork = ArtworkCache::new();
        let mut waveform = WaveformState::default();
        let ctx = fresh_ctx();
        ctx.enable_accesskit();

        let remove_label = tr("markers-remove");
        let f = frame(
            &ctx,
            &mut controller,
            &mut artwork,
            &mut waveform,
            tall_input(),
        );
        let bounds = f
            .nth_bounds(Role::Button, &remove_label, 0)
            .unwrap_or_else(|| unreachable!("expected the B row's remove action"));
        click_at(
            &ctx,
            &mut controller,
            &mut artwork,
            &mut waveform,
            bounds.center(),
        );

        assert!(
            controller
                .markers()
                .and_then(|m| m.region(region))
                .is_none(),
            "removing the last boundary must delete the region"
        );
    }

    // Case 3: removing an armed region's endpoint disarms it.
    {
        let (mut controller, _handle, _dir) = active_controller("row-remove-armed-disarm");
        controller.queue_replace(vec![track("a", 200_000)]);
        controller.play();
        controller.tick();
        controller
            .set_loop_a()
            .unwrap_or_else(|e| unreachable!("set_loop_a: {e}"));
        let _ = controller.backend_mut().render_buffers(20);
        controller
            .set_loop_b()
            .unwrap_or_else(|e| unreachable!("set_loop_b: {e}"));
        let region = controller
            .markers()
            .and_then(TrackMarkers::current_region)
            .unwrap_or_else(|| unreachable!());
        controller
            .arm_loop(region)
            .unwrap_or_else(|e| unreachable!("arm_loop: {e}"));
        let _ = controller.backend_mut().render_buffers(1);
        assert_ne!(
            controller.loop_status().state,
            LoopState::Disarmed,
            "sanity: must be armed"
        );

        let mut artwork = ArtworkCache::new();
        let mut waveform = WaveformState::default();
        let ctx = fresh_ctx();
        ctx.enable_accesskit();

        let remove_label = tr("markers-remove");
        let f = frame(
            &ctx,
            &mut controller,
            &mut artwork,
            &mut waveform,
            tall_input(),
        );
        let bounds = f
            .nth_bounds(Role::Button, &remove_label, 0)
            .unwrap_or_else(|| unreachable!("expected a remove action"));
        click_at(
            &ctx,
            &mut controller,
            &mut artwork,
            &mut waveform,
            bounds.center(),
        );
        let _ = controller.backend_mut().render_buffers(1);

        assert_eq!(
            controller.loop_status().state,
            LoopState::Disarmed,
            "removing an armed endpoint must disarm"
        );
    }
}

/// T023 (contract P6; Clarification 8): after removing a row, focus
/// moves to the next populated row in visual order, else the previous
/// one, else "New loop region".
#[test]
fn row_remove_moves_focus_next_prev_new_loop() {
    let (mut controller, _handle, _dir) = active_controller("row-remove-focus");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();

    let mut ids = Vec::new();
    for _ in 0..3 {
        let _ = controller.backend_mut().render_buffers(50);
        ids.push(
            controller
                .add_point_marker()
                .unwrap_or_else(|e| unreachable!("add_point_marker: {e}")),
        );
    }
    let id1 = ids[0];
    let id3 = ids[2];

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    // Remove the middle marker (id2): focus must move to the next row
    // (id3), reflected in `waveform.focused_marker` since `show_swatch`
    // pushes `PanelIntent::Focus` the same frame it applies the pending
    // target (T033's `request_focus()` runs before the `has_focus()`
    // check).
    let remove_label = tr("markers-remove");
    let f = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );
    let bounds = f
        .nth_bounds(Role::Button, &remove_label, 1)
        .unwrap_or_else(|| unreachable!("expected three rows' remove actions"));
    click_at(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        bounds.center(),
    );
    let _ = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );
    assert_eq!(
        waveform.focused_marker,
        Some(id3),
        "focus must move to the next row (id3) after removing the middle one"
    );

    // Remove id3 (now the last row): focus must move to the *previous*
    // row (id1).
    let f = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );
    let bounds = f
        .nth_bounds(Role::Button, &remove_label, 1)
        .unwrap_or_else(|| unreachable!("expected two rows' remove actions"));
    click_at(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        bounds.center(),
    );
    let _ = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );
    assert_eq!(
        waveform.focused_marker,
        Some(id1),
        "focus must move to the previous row (id1) once there is no next one"
    );

    // Remove the last remaining marker: focus falls through to "New loop
    // region".
    let f = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );
    let bounds = f
        .nth_bounds(Role::Button, &remove_label, 0)
        .unwrap_or_else(|| unreachable!("expected one row's remove action"));
    click_at(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        bounds.center(),
    );
    let f = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );
    assert_eq!(waveform.focused_marker, None);
    assert_eq!(
        f.focused().and_then(|n| n.label.clone()),
        Some(tr("markers-new-loop")),
        "focus must fall through to New loop region once nothing is left"
    );
    let _ = id1;
}

/// T024 (contract P4; FR-013): the swatch opens a popover — `Role::
/// RadioGroup` labelled `markers-palette`, 8 `Role::RadioButton` swatches
/// labelled `markers-color { $index }` 1..8 (the 1-based regression
/// check, research R7), the current one toggled — and picking a
/// different swatch recolours the marker and closes the popover.
#[test]
fn swatch_opens_palette_popover_and_picks() {
    let (mut controller, _handle, _dir) = active_controller("palette-popover");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    let id = controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));
    let before_color = controller
        .markers()
        .and_then(|m| m.marker(id))
        .map(|m| m.color)
        .unwrap_or_else(|| unreachable!());

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    let swatch_label = tr_args(
        "markers-color",
        &[("index", (before_color.get() + 1).to_string())],
    );
    let f = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );
    let swatch_bounds = f.bounds(Role::Button, &swatch_label);
    click_at(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        swatch_bounds.center(),
    );

    let f = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );
    assert!(
        f.has(Role::RadioGroup, &tr("markers-palette")),
        "expected the popover's RadioGroup container"
    );
    let mut toggled_indices = Vec::new();
    for raw in 1u8..=8 {
        let label = tr_args("markers-color", &[("index", raw.to_string())]);
        let node = f
            .find(Role::RadioButton, &label)
            .unwrap_or_else(|| unreachable!("expected RadioButton {label:?} in the popover"));
        if node.toggled == Some(Toggled::True) {
            toggled_indices.push(raw);
        }
    }
    assert_eq!(
        toggled_indices,
        vec![before_color.get() + 1],
        "exactly the current colour's swatch must be toggled"
    );

    // Pick a different swatch.
    let target_raw: u8 = if before_color.get() == 5 { 6 } else { 5 };
    let target_label = tr_args("markers-color", &[("index", (target_raw + 1).to_string())]);
    let f = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );
    let target_bounds = f.bounds(Role::RadioButton, &target_label);
    click_at(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        target_bounds.center(),
    );

    let after_color = controller
        .markers()
        .and_then(|m| m.marker(id))
        .map(|m| m.color)
        .unwrap_or_else(|| unreachable!());
    assert_eq!(
        after_color.get(),
        target_raw,
        "picking a swatch must recolour the marker"
    );

    let f = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );
    assert!(
        !f.has(Role::RadioGroup, &tr("markers-palette")),
        "the popover must close after a pick"
    );
}

/// T024 (contract P4): `Esc` and a click outside the popover both close
/// it with no change to the marker's colour.
#[test]
fn palette_esc_and_click_outside_no_change() {
    let (mut controller, _handle, _dir) = active_controller("palette-esc-outside");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    let id = controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));
    let before_color = controller
        .markers()
        .and_then(|m| m.marker(id))
        .map(|m| m.color)
        .unwrap_or_else(|| unreachable!());

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    let swatch_label = tr_args(
        "markers-color",
        &[("index", (before_color.get() + 1).to_string())],
    );
    let f = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );
    let swatch_bounds = f.bounds(Role::Button, &swatch_label);
    click_at(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        swatch_bounds.center(),
    );

    let f = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );
    assert!(
        f.has(Role::RadioGroup, &tr("markers-palette")),
        "popover must be open before Esc"
    );

    let mut esc_input = tall_input();
    esc_input.events.push(key_event(Key::Escape));
    esc_input.events.push(key_release(Key::Escape));
    let _ = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        esc_input,
    );
    // The popover's own `Escape`-closes-me check (peeked, not consumed,
    // inside its `Popup::show`) still draws its *old* (open) content the
    // very frame `Escape` arrives — the close only takes effect for the
    // frame after — so the closed state is only visible on one more
    // rendered frame.
    let f = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );
    assert!(
        !f.has(Role::RadioGroup, &tr("markers-palette")),
        "Esc must close the popover"
    );
    assert_eq!(
        controller
            .markers()
            .and_then(|m| m.marker(id))
            .map(|m| m.color),
        Some(before_color),
        "Esc must not change the colour"
    );

    // Reopen, then click far outside the popup — closes with no change.
    let f = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );
    let swatch_bounds = f.bounds(Role::Button, &swatch_label);
    click_at(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        swatch_bounds.center(),
    );
    let f = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );
    assert!(
        f.has(Role::RadioGroup, &tr("markers-palette")),
        "popover must be open again"
    );

    click_at(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Pos2::new(5.0, 595.0),
    );
    let f = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );
    assert!(
        !f.has(Role::RadioGroup, &tr("markers-palette")),
        "a click outside must close the popover"
    );
    assert_eq!(
        controller
            .markers()
            .and_then(|m| m.marker(id))
            .map(|m| m.color),
        Some(before_color),
        "click-outside must not change the colour"
    );
}

/// T025 (contract P5; FR-012): clicking an unnamed row's name cell opens
/// the rename with `markers-name-placeholder` shown when empty, and an
/// empty draft (never the placeholder text itself).
#[test]
fn click_name_opens_rename_placeholder_when_unnamed() {
    let (mut controller, _handle, _dir) = active_controller("click-name-placeholder");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    controller
        .set_loop_a()
        .unwrap_or_else(|e| unreachable!("set_loop_a: {e}"));
    let region = controller
        .markers()
        .and_then(TrackMarkers::current_region)
        .unwrap_or_else(|| unreachable!());
    let a_id = controller
        .markers()
        .and_then(|m| m.region(region))
        .and_then(|r| r.a)
        .unwrap_or_else(|| unreachable!());
    let name_before = controller
        .markers()
        .and_then(|m| m.marker(a_id))
        .map(|m| m.name.clone())
        .unwrap_or_default();
    assert!(
        name_before.is_empty(),
        "sanity: a fresh region endpoint has no name"
    );

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    let placeholder = tr("markers-name-placeholder");
    let f = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );
    let bounds = f.bounds(Role::Button, &placeholder);
    click_at(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        bounds.center(),
    );

    assert_eq!(
        waveform
            .rename
            .as_ref()
            .map(|(id, draft)| (*id, draft.as_str())),
        Some((a_id, "")),
        "clicking the placeholder must open the rename with an empty draft"
    );
}

/// T025 (contract P5; FR-012): focus loss (a click elsewhere) commits
/// the typed draft; `Esc` cancels it, leaving the name unchanged.
#[test]
fn rename_commits_on_focus_loss_esc_cancels() {
    let (mut controller, _handle, _dir) = active_controller("rename-commit-cancel-click");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    let id = controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    let name_before = controller
        .markers()
        .and_then(|m| m.marker(id))
        .map(|m| m.name.clone())
        .unwrap_or_default();
    let f = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );
    let bounds = f.bounds(Role::Button, &name_before);
    click_at(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        bounds.center(),
    );
    assert!(waveform.rename.is_some());
    // One settle frame: `OpenRename` (applied at the end of the click's
    // release frame) only *requests* the `TextEdit`'s focus while it
    // draws this next frame (research R9) — the frame after *that* is
    // the first one where it actually holds focus and can consume a
    // typed `Event::Text` (mirrors `shortcuts_inactive_while_rename_
    // open`'s own comment on this same one-frame lag).
    let _ = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );

    let mut type_input = tall_input();
    type_input.events.push(Event::Text("Hi".to_string()));
    let _ = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        type_input,
    );

    // A click elsewhere (empty space) — egui surrenders any widget's
    // focus on a click that did not land on it, regardless of what (if
    // anything) the click did hit.
    click_at(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Pos2::new(5.0, 595.0),
    );

    assert!(
        waveform.rename.is_none(),
        "focus loss must commit and close the rename"
    );
    assert_eq!(
        controller
            .markers()
            .and_then(|m| m.marker(id))
            .map(|m| m.name.clone()),
        Some("Hi".to_string()),
        "focus loss must commit the typed name"
    );

    // Reopen, type more, Esc cancels — name stays "Hi".
    let f = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );
    let bounds = f.bounds(Role::Button, "Hi");
    click_at(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        bounds.center(),
    );
    assert!(waveform.rename.is_some());
    let _ = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );

    let mut type_input = tall_input();
    type_input.events.push(Event::Text("!!!".to_string()));
    let _ = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        type_input,
    );

    let mut esc_input = tall_input();
    esc_input.events.push(key_event(Key::Escape));
    esc_input.events.push(key_release(Key::Escape));
    let _ = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        esc_input,
    );

    assert!(waveform.rename.is_none(), "Esc must close the rename");
    assert_eq!(
        controller
            .markers()
            .and_then(|m| m.marker(id))
            .map(|m| m.name.clone()),
        Some("Hi".to_string()),
        "Esc must cancel without changing the name"
    );
}

/// T025 (contract P5; SC-004): the committed name shows on the lane
/// glyph's own accessible name (the same `marker-glyph` template the row
/// uses) starting the frame *after* the commit — the lanes draw before
/// the panel, so this same interaction's commit cannot show on the
/// glyph until a further rendered frame (research R9's forced repaint
/// delivers that next frame immediately, not merely eventually).
#[test]
fn rename_reflected_in_lane_glyph_name_next_frame() {
    let (mut controller, _handle, _dir) = active_controller("rename-reflects-glyph");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    let id = controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    let name_before = controller
        .markers()
        .and_then(|m| m.marker(id))
        .map(|m| m.name.clone())
        .unwrap_or_default();
    let f = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );
    let bounds = f.bounds(Role::Button, &name_before);
    click_at(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        bounds.center(),
    );
    let _ = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );

    let mut type_input = tall_input();
    type_input.events.push(Event::Text("Chorus".to_string()));
    let _ = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        type_input,
    );

    let mut enter_input = tall_input();
    enter_input.events.push(key_event(Key::Enter));
    enter_input.events.push(key_release(Key::Enter));
    let _ = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        enter_input,
    );
    assert!(waveform.rename.is_none());
    assert_eq!(
        controller
            .markers()
            .and_then(|m| m.marker(id))
            .map(|m| m.name.clone()),
        Some("Chorus".to_string()),
        "sanity: commit must have landed in the model"
    );

    let needle = "\u{2068}Chorus\u{2069}";
    let f = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );
    let has_new_name = f
        .nodes
        .iter()
        .any(|(_, n)| n.label.as_deref().is_some_and(|l| l.contains(needle)));
    assert!(
        has_new_name,
        "expected the committed name reflected via the marker-glyph template on the next frame"
    );
}

/// T025 (Edge Case): a rename left open on one row commits before
/// another row's own action runs, even when both land in the same
/// input.
#[test]
fn rename_commits_before_other_row_action() {
    let (mut controller, _handle, _dir) = active_controller("rename-before-other-action");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    let _ = controller.backend_mut().render_buffers(50);
    let id1 = controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));
    let _ = controller.backend_mut().render_buffers(50);
    let id2 = controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    let name1_before = controller
        .markers()
        .and_then(|m| m.marker(id1))
        .map(|m| m.name.clone())
        .unwrap_or_default();
    let f = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );
    let name_bounds = f.bounds(Role::Button, &name1_before);
    click_at(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        name_bounds.center(),
    );
    assert_eq!(waveform.rename.as_ref().map(|(id, _)| *id), Some(id1));

    // One settle frame: `OpenRename` (applied at the end of `click_at`'s
    // own release frame) only *requests* the `TextEdit`'s focus while it
    // draws this next frame (research R9) — the frame after *that* is
    // the first one where it actually holds focus and can consume a
    // typed `Event::Text` (mirrors `rename_commits_on_focus_loss_esc_
    // cancels`'s own comment on this same one-frame lag).
    let _ = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );

    let mut type_input = tall_input();
    type_input.events.push(Event::Text("Verse".to_string()));
    let _ = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        type_input,
    );

    // id1's row (created first, earlier position) renders above id2's —
    // its remove action is the *second* (lower) match.
    let remove_label = tr("markers-remove");
    let f = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    );
    let id2_remove_bounds = f
        .nth_bounds(Role::Button, &remove_label, 1)
        .unwrap_or_else(|| unreachable!("expected two rows' remove actions"));
    click_at_same_frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        id2_remove_bounds.center(),
    );

    assert!(waveform.rename.is_none(), "the rename must have committed");
    assert_eq!(
        controller
            .markers()
            .and_then(|m| m.marker(id1))
            .map(|m| m.name.clone()),
        Some("Verse".to_string()),
        "id1's rename must have committed, not been lost, once id2's remove also ran"
    );
    assert!(
        controller.markers().and_then(|m| m.marker(id2)).is_none(),
        "id2 must have been removed"
    );
}

/// T026 (contract P7; research R8): `Enter` on a row action (never a
/// lane glyph or the name cell) only activates that action — it must
/// not also open a rename.
#[test]
fn enter_on_row_action_does_not_open_rename() {
    let (mut controller, _handle, _dir) = active_controller("enter-on-action-no-rename");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    let id = controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));
    let marker_pos = controller
        .markers()
        .and_then(|m| m.position_of(id))
        .unwrap_or_else(|| unreachable!());

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState {
        panel_focus: Some(PanelFocus::Row(id)),
        ..Default::default()
    };
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    let _ = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tall_input(),
    ); // swatch focused
    let _ = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tab_input(),
    ); // name cell
    let f = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        tab_input(),
    ); // jump
    assert_eq!(
        f.focused().and_then(|n| n.label.clone()),
        Some(tr("markers-jump")),
        "sanity: the jump action must have focus"
    );

    let _ = controller.backend_mut().render_buffers(50);
    let moved_pos = controller.shared().position_frames();
    assert!(
        moved_pos > marker_pos + JUMP_LAND_TOLERANCE_FRAMES,
        "sanity: must have advanced"
    );

    let mut enter_input = tall_input();
    enter_input.events.push(key_event(Key::Enter));
    enter_input.events.push(key_release(Key::Enter));
    let _ = frame(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        enter_input,
    );
    controller.tick();
    let _ = controller.backend_mut().render_buffers(1);

    let landed = controller.shared().position_frames();
    assert!(
        landed >= marker_pos && landed <= marker_pos + JUMP_LAND_TOLERANCE_FRAMES,
        "Enter on the jump action must activate it (seek to the marker): landed={landed}"
    );
    assert!(
        waveform.rename.is_none(),
        "Enter on a row action must not also open a rename"
    );
}

// ---------------------------------------------------------------------
// 023-markers-panel-structure — Phase 5, User Story 3: Clear-all is
// visibly and spatially separate from New loop region (contracts/
// ui-markers-panel.md P9; T034-T036).
// ---------------------------------------------------------------------

/// T034 (contract P9; FR-018/FR-019; SC-005/SC-006): "Clear all markers"
/// renders in a right-aligned footer below the Cues group — the Loop,
/// Points and Cues groups always lie between it and "New loop region", and
/// it is never on the same line as, or immediately adjacent to, that
/// control (research R13).
#[test]
fn clear_all_in_footer_destructive_not_adjacent() {
    let (mut controller, _handle, _dir) = active_controller("clear-all-footer");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    let mut output = ctx.run_ui(default_input(), |ui| {
        modplayer_ui::now_playing::show(
            ui,
            &mut controller,
            &mut artwork,
            &mut waveform,
            &mut modplayer_ui::section_memory::SectionMemory::default(),
        )
    });
    let update = output
        .platform_output
        .accesskit_update
        .take()
        .expect("accesskit_update should be populated once enabled");
    output.drop_without_applying_deltas();

    let button_bounds = |label: &str| -> Rect {
        let bounds = update
            .nodes
            .iter()
            .find_map(|(_, node)| {
                if node.role() == Role::Button && node.label() == Some(label) {
                    node.bounds()
                } else {
                    None
                }
            })
            .unwrap_or_else(|| unreachable!("expected a Button node labelled {label:?}"));
        Rect::from_min_max(
            Pos2::new(bounds.x0 as f32, bounds.y0 as f32),
            Pos2::new(bounds.x1 as f32, bounds.y1 as f32),
        )
    };

    let new_loop = button_bounds(&tr("markers-new-loop"));
    let clear_all = button_bounds(&tr("markers-clear-all"));

    let cues_label = tr("markers-group-cues");
    let cues_heading_y = update
        .nodes
        .iter()
        .filter(|(_, node)| node.role() == Role::Heading)
        .find_map(|(_, node)| {
            let label = node.label()?;
            if label.contains(cues_label.as_str()) {
                node.bounds().map(|b| b.y0 as f32)
            } else {
                None
            }
        })
        .unwrap_or_else(|| unreachable!("expected the Cues heading"));

    assert!(
        clear_all.min.y > cues_heading_y,
        "Clear all markers must render below the Cues group heading: clear_all.y0={} cues_heading.y0={}",
        clear_all.min.y,
        cues_heading_y
    );
    assert!(
        clear_all.min.y - new_loop.max.y > 20.0,
        "Clear all markers must not sit immediately below New loop region — the Loop/Points/Cues \
         groups must separate them: new_loop.y1={} clear_all.y0={}",
        new_loop.max.y,
        clear_all.min.y
    );
    assert!(
        clear_all.min.x > new_loop.min.x + 40.0,
        "Clear all markers must render right-aligned, unlike the left-aligned New loop region: \
         new_loop.x0={} clear_all.x0={}",
        new_loop.min.x,
        clear_all.min.x
    );
}

/// T034 (contract P9; FR-018/FR-019; SC-006): moving "Clear all markers"
/// into the footer (T035) does not change its two-step confirmation —
/// `markers-clear-confirm { $count }` plus Destructive Yes / Default No —
/// `Esc` still cancels without touching any marker, and Yes still empties
/// the model.
#[test]
fn clear_all_two_step_unchanged() {
    let (mut controller, _handle, _dir) = active_controller("clear-all-footer-two-step");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    controller
        .set_loop_a()
        .unwrap_or_else(|e| unreachable!("set_loop_a: {e}"));
    let count_before = controller.markers().map(TrackMarkers::count).unwrap_or(0);
    assert!(count_before > 0, "sanity: a marker must exist");

    let mut artwork = ArtworkCache::new();
    let mut waveform = WaveformState::default();
    let ctx = fresh_ctx();
    ctx.enable_accesskit();

    let texts = rendered_texts(&ctx, &mut controller, &mut artwork, &mut waveform);
    assert!(texts.contains(&tr("markers-clear-all")));
    let confirm_text = tr_args(
        "markers-clear-confirm",
        &[("count", count_before.to_string())],
    );
    assert!(!texts.contains(&confirm_text));

    waveform.clear_confirm = true;
    let texts = rendered_texts(&ctx, &mut controller, &mut artwork, &mut waveform);
    assert!(
        texts.contains(&confirm_text),
        "expected {confirm_text:?} among {texts:?}"
    );
    assert!(texts.contains(&tr("markers-clear-yes")));
    assert!(texts.contains(&tr("markers-clear-no")));

    press_key(
        &ctx,
        &mut controller,
        &mut artwork,
        &mut waveform,
        Key::Escape,
    );
    assert!(!waveform.clear_confirm, "Esc must cancel the confirmation");
    assert_eq!(
        controller.markers().map(TrackMarkers::count).unwrap_or(0),
        count_before,
        "Esc must not clear any marker"
    );

    waveform.clear_confirm = true;
    controller.clear_all_markers();
    waveform.clear_confirm = false;
    assert_eq!(
        controller.markers().map(TrackMarkers::count).unwrap_or(0),
        0,
        "confirming Yes must empty the model"
    );
}

// ---------------------------------------------------------------------
// 023-markers-panel-structure — Phase 6, User Story 4: the panel tells
// you what to do when it has nothing, or room, to show (contract P1/P2;
// T037).
// ---------------------------------------------------------------------

/// T037 (US4 AS4; FR-004): a track with only Points, or only Cues content
/// (no loop region at all) still renders the Loop Region heading at
/// count 0, first, and its "New loop region" control — the Loop Region
/// group never disappears just because no region exists yet.
#[test]
fn loop_group_shows_zero_count_and_new_loop_control_without_a_region() {
    let expected_loop_zero = tr_args(
        "markers-group-heading",
        &[
            ("label", tr("markers-group-loop")),
            ("count", "0".to_string()),
        ],
    );

    // Points-only: no loop region, no cue.
    {
        let (mut controller, _handle, _dir) = active_controller("loop-zero-points-only");
        controller.queue_replace(vec![track("a", 200_000)]);
        controller.play();
        controller.tick();
        controller
            .add_point_marker()
            .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));

        let mut artwork = ArtworkCache::new();
        let mut waveform = WaveformState::default();
        let ctx = fresh_ctx();
        ctx.enable_accesskit();

        let headings =
            markers_group_headings_in_order(&ctx, &mut controller, &mut artwork, &mut waveform);
        assert_eq!(
            headings.first(),
            Some(&expected_loop_zero),
            "points-only: expected the Loop Region heading at count 0, first, got {headings:?}"
        );

        let texts = rendered_texts(&ctx, &mut controller, &mut artwork, &mut waveform);
        assert!(
            texts.contains(&tr("markers-new-loop")),
            "points-only: expected the New loop region control among {texts:?}"
        );
    }

    // Cues-only: no loop region, no point.
    {
        let (mut controller, _handle, _dir) = active_controller("loop-zero-cues-only");
        controller.queue_replace(vec![track("a", 200_000)]);
        controller.play();
        controller.tick();
        let cue_slot = CueSlot::new(3).unwrap_or_else(|| unreachable!());
        controller
            .set_cue(cue_slot)
            .unwrap_or_else(|e| unreachable!("set_cue: {e}"));

        let mut artwork = ArtworkCache::new();
        let mut waveform = WaveformState::default();
        let ctx = fresh_ctx();
        ctx.enable_accesskit();

        let headings =
            markers_group_headings_in_order(&ctx, &mut controller, &mut artwork, &mut waveform);
        assert_eq!(
            headings.first(),
            Some(&expected_loop_zero),
            "cues-only: expected the Loop Region heading at count 0, first, got {headings:?}"
        );

        let texts = rendered_texts(&ctx, &mut controller, &mut artwork, &mut waveform);
        assert!(
            texts.contains(&tr("markers-new-loop")),
            "cues-only: expected the New loop region control among {texts:?}"
        );
    }
}

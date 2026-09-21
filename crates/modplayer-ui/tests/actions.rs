// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! Dispatcher/invoker and regression tests (007, US1, contracts/
//! ui-actions.md §7, quickstart.md's pinning table): the two-pass chord
//! matcher, FR-019 precedence/repeat rules, FR-018 scopes, and the
//! inherited-binding regression suite that proves markers/loop/cues/nav
//! (001/004/006) behave exactly as before the dispatcher replaced their
//! ad-hoc handlers (plan.md Design Note 1).
//!
//! Every test here drives `modplayer_ui::actions::dispatch`/`invoke`
//! directly against a bare `egui::Context` — no `now_playing::show` (or
//! any other screen) is rendered, because the dispatcher's own contract
//! (contracts/ui-actions.md §2) depends only on `ctx`'s focus/input state
//! and a `FocusClaims` value, never on what a screen actually draws.
//! `crates/modplayer-ui/tests/markers.rs` and `tests/now_playing.rs`
//! cover the same dispatcher wired through the real screens (T047).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use egui::{Context, Event, Id, Key as EguiKey, Modifiers, Pos2, RawInput, Rect};
use modplayer_audio_io::{FakeBackend, FakeDevice};
use modplayer_audio_source::{Availability, TrackId, TrackRef};
use modplayer_audio_source_synthetic::{ScriptedHost, ScriptedHostHandle};
use modplayer_core::actions::{
    ActionId, Chord, HostAction, KEY_NAMES, KeyName, Mods, PluginActionId, ScopeState,
};
use modplayer_core::markers::{CueSlot, TrackMarkers};
use modplayer_core::notifications::KEY_EFFECTS_NO_TIME_STRETCH;
use modplayer_core::plugins::{Lifecycle, PluginId};
use modplayer_core::settings::SettingsStore;
use modplayer_core::{Intent, LoopState, PlaybackController};
use modplayer_effects::catalog::NodeKind;
use modplayer_engine::{BufferPreset, DeviceId, FrameCount, SampleRate};
use modplayer_ui::actions::{Claim, FocusClaims, Invocation};
use modplayer_ui::waveform::WaveformState;
use modplayer_ui::{Section, Shell, actions};

/// How many frames a cue jump's landing position may sit past the cue it
/// targeted (mirrors `markers.rs::JUMP_LAND_TOLERANCE_FRAMES`): landing a
/// seek requires one render call, which itself advances playback by that
/// render's own frame count.
const JUMP_LAND_TOLERANCE_FRAMES: u64 = 512;

// -- Fixtures (mirrors markers.rs/now_playing.rs) --------------------------

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-ui-actions-{label}-{}-{unique}",
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
/// new`'s one synchronous read of it (mirrors `markers.rs`'s own lock:
/// 006 US2's `sync_marker_attachment`/debounced flush now runs on every
/// `dispatch`, so an unisolated controller would read/write the real
/// per-user track-state directory).
static TRACK_STATE_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Both temp dirs `active_controller` allocates (settings, track-state);
/// kept alive together so either can be dropped only once the test itself
/// is done with the controller.
struct TestDirs(#[allow(dead_code)] TempDir, #[allow(dead_code)] TempDir);

/// A controller over a confirmed device, ready to play (mirrors
/// `markers.rs`/`now_playing.rs`'s own `active_controller`).
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
    controller.launch();
    controller.confirm_device(
        DeviceId::new("dev-1").unwrap_or_else(|| unreachable!()),
        BufferPreset::Balanced,
    );
    controller.set_playback_permitted(true, None);
    controller.tick();
    (controller, handle, TestDirs(dir, track_state_dir))
}

/// `active_controller` with every bundled plugin disabled before it can
/// touch the chain. The tempo-step tests below add their own
/// `TimeStretch` node and assert on it, but `tempo_step` acts on the
/// chain's *first* time-stretch node — and Key & Tempo (013), launched
/// by `launch()`, creates its own pitch/stretch pair asynchronously from
/// `ready_ack`. Whichever node lands first wins, which made those tests
/// timing-dependent. Disabling the plugins stops their threads; any node
/// a plugin already managed to create is *orphaned* by that teardown
/// (chain.md L7e), not removed, so the drain removes whatever is left
/// once no plugin can add more, and returns with an empty chain.
fn active_controller_without_bundled_plugins(
    label: &str,
) -> (
    PlaybackController<FakeBackend, ScriptedHost>,
    ScriptedHostHandle,
    TestDirs,
) {
    let (mut controller, handle, dirs) = active_controller(label);
    let ids: Vec<PluginId> = controller
        .plugins_mut()
        .records()
        .iter()
        .map(|r| r.id)
        .collect();
    for id in ids {
        controller.plugin_disable(id);
    }
    // A plugin's `chain_add_node` RPC can already be queued when its
    // thread is stopped and still be applied on a later tick, so keep
    // removing until the chain has stayed empty across several ticks
    // with every plugin down.
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut quiet_ticks = 0;
    loop {
        controller.tick();
        let leftovers: Vec<_> = controller.chain().nodes().iter().map(|n| n.id).collect();
        for id in leftovers {
            let _ = controller.chain_remove_node(id);
        }
        let all_down = controller
            .plugins_mut()
            .records()
            .iter()
            .all(|r| !matches!(r.lifecycle, Lifecycle::Loading | Lifecycle::Active));
        if all_down && controller.chain().nodes().is_empty() {
            quiet_ticks += 1;
            if quiet_ticks >= 5 {
                break;
            }
        } else {
            quiet_ticks = 0;
        }
        assert!(
            Instant::now() < deadline,
            "bundled plugins must go down and leave the chain empty"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    (controller, handle, dirs)
}

fn default_input() -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
        ..Default::default()
    }
}

// -- Event / dispatch helpers -----------------------------------------------

fn key(key: EguiKey, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

fn key_release(key: EguiKey) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

/// A physical-fallback event shape (research R3): `key` is the *logical*
/// key the layout produced, `physical` is the raw key underneath it —
/// used to exercise `dispatch`'s pass-2 matcher (e.g. `Shift+1` arriving
/// as logical `Exclamationmark` over physical `Num1`).
fn key_physical(key: EguiKey, physical: EguiKey, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: Some(physical),
        pressed: true,
        repeat: false,
        modifiers,
    }
}

/// `KeyName` -> `egui::Key` via egui's own `from_name` (the exact inverse
/// of `Key::name()`, which `KEY_NAMES`/`KeyName::parse` mirror, research
/// R2) — lets a test build an event straight from a catalog chord without
/// a second, hand-written translation table.
fn egui_key(name: KeyName) -> EguiKey {
    EguiKey::from_name(name.as_str()).unwrap_or_else(|| unreachable!("{name:?} has no egui::Key"))
}

fn egui_mods(mods: Mods) -> Modifiers {
    Modifiers {
        command: mods.primary,
        shift: mods.shift,
        alt: mods.alt,
        ..Modifiers::NONE
    }
}

/// Run one frame with `events`, dispatching and invoking against
/// `registry`'s owner (`controller.actions()`) — mirrors what `App::ui`
/// does every frame (contracts/ui-actions.md §1), minus any widget draw.
/// Returns the resulting `Invocation`s so a caller can assert on the
/// dispatcher's own decision, not just its downstream effect.
fn frame(
    ctx: &Context,
    events: Vec<Event>,
    claims: &FocusClaims,
    scope: &ScopeState,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    shell: &mut Shell,
    waveform: &mut WaveformState,
) -> Vec<Invocation> {
    let mut input = default_input();
    input.events = events;
    let mut invocations = Vec::new();
    let output = ctx.run_ui(input, |ui| {
        let frame_ctx = ui.ctx().clone();
        invocations = actions::dispatch(&frame_ctx, claims, controller.actions(), scope);
        for inv in invocations.clone() {
            actions::invoke(inv, controller, shell, waveform, &frame_ctx);
        }
    });
    output.drop_without_applying_deltas();
    invocations
}

/// A single key press *and* its matching release, in one frame — the
/// shape every real key press takes (a later press of the same key on
/// the same `Context`, with no release in between, is auto-marked
/// `repeat: true` by egui's own `InputState::begin_pass`, which several
/// tests below rely on deliberately; `press` avoids that footgun for
/// every other test by always releasing).
fn press(
    ctx: &Context,
    ev: Event,
    claims: &FocusClaims,
    scope: &ScopeState,
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    shell: &mut Shell,
    waveform: &mut WaveformState,
) -> Vec<Invocation> {
    let Event::Key { key: k, .. } = ev else {
        unreachable!("press() only takes Event::Key");
    };
    frame(
        ctx,
        vec![ev, key_release(k)],
        claims,
        scope,
        controller,
        shell,
        waveform,
    )
}

fn app_scope() -> ScopeState {
    ScopeState {
        now_playing_shown: false,
        marker_focused: false,
    }
}

fn now_playing_scope() -> ScopeState {
    ScopeState {
        now_playing_shown: true,
        marker_focused: false,
    }
}

fn marker_focused_scope() -> ScopeState {
    ScopeState {
        now_playing_shown: true,
        marker_focused: true,
    }
}

// -- T045: the inherited-binding regression suite (FR-005, FR-017) ---------

/// Every 001/004/006 binding this feature inherits unchanged, driven
/// through the 007 dispatcher instead of the ad-hoc handlers it replaced
/// (contracts/ui-actions.md §7, SC-007): navigation + focus-search,
/// markers (`I`/`O`/`L`/`M`), cues (`1`-`8`, `Shift+1`-`Shift+8`, both the
/// logical-fallback and physical-fallback event shapes), and the
/// marker-focus nudge arrows.
#[test]
fn inherited_bindings_produce_identical_controller_calls() {
    // -- Navigation (Primary+1..5) + focus-search (Primary+F, Slash) --
    {
        let (mut controller, _handle, _dirs) = active_controller("inherited-nav");
        controller.queue_replace(vec![track("a", 200_000)]);
        let mut shell = Shell::default();
        let mut waveform = WaveformState::default();
        let ctx = Context::default();
        let claims = FocusClaims::default();
        let scope = app_scope();

        let nav_cases = [
            (EguiKey::Num1, Section::Library),
            (EguiKey::Num2, Section::Search),
            (EguiKey::Num3, Section::NowPlaying),
            (EguiKey::Num4, Section::Plugins),
            (EguiKey::Num5, Section::Settings),
        ];
        for (digit, expected) in nav_cases {
            shell.section = Section::Plugins; // a value none of the cases starts on
            press(
                &ctx,
                key(digit, Modifiers::COMMAND),
                &claims,
                &scope,
                &mut controller,
                &mut shell,
                &mut waveform,
            );
            assert_eq!(
                shell.section, expected,
                "Primary+{digit:?} must navigate to {expected:?}"
            );
        }

        for (focus_key, modifiers) in [
            (EguiKey::F, Modifiers::COMMAND),
            (EguiKey::Slash, Modifiers::NONE),
        ] {
            shell.section = Section::Library;
            shell.focus_search_requested = false;
            press(
                &ctx,
                key(focus_key, modifiers),
                &claims,
                &scope,
                &mut controller,
                &mut shell,
                &mut waveform,
            );
            assert_eq!(
                shell.section,
                Section::Search,
                "{focus_key:?} must focus search"
            );
            assert!(
                shell.focus_search_requested,
                "{focus_key:?} must request search-box focus"
            );
        }
    }

    // -- Markers: I, O, L, M --
    {
        let (mut controller, _handle, _dirs) = active_controller("inherited-markers");
        controller.queue_replace(vec![track("a", 200_000)]);
        controller.play();
        controller.tick();
        let mut shell = Shell::default();
        let mut waveform = WaveformState::default();
        let ctx = Context::default();
        let claims = FocusClaims::default();
        let scope = now_playing_scope();

        let expected_a = controller.shared().position_frames();
        press(
            &ctx,
            key(EguiKey::I, Modifiers::NONE),
            &claims,
            &scope,
            &mut controller,
            &mut shell,
            &mut waveform,
        );
        let region = controller
            .markers()
            .and_then(TrackMarkers::current_region)
            .expect("I must create a region");
        let a_id = controller
            .markers()
            .and_then(|m| m.region(region))
            .and_then(|r| r.a)
            .expect("A endpoint must exist after I");
        assert_eq!(
            controller.markers().and_then(|m| m.position_of(a_id)),
            Some(expected_a)
        );

        let _ = controller.backend_mut().render_buffers(5);
        let expected_b = controller.shared().position_frames();
        press(
            &ctx,
            key(EguiKey::O, Modifiers::NONE),
            &claims,
            &scope,
            &mut controller,
            &mut shell,
            &mut waveform,
        );
        let markers = controller.markers().expect("markers must exist");
        let (a_final, b_final) = markers
            .region(region)
            .and_then(|r| r.span(markers))
            .expect("region must be complete after O");
        assert_eq!(a_final, expected_a);
        assert_eq!(b_final, expected_b);

        let _ = controller.backend_mut().render_buffers(1);
        press(
            &ctx,
            key(EguiKey::L, Modifiers::NONE),
            &claims,
            &scope,
            &mut controller,
            &mut shell,
            &mut waveform,
        );
        assert!(
            controller
                .markers()
                .and_then(|m| m.region(region))
                .is_some_and(|r| r.armed),
            "L must arm the now-complete region"
        );

        let count_before_m = controller.markers().map(TrackMarkers::count).unwrap_or(0);
        press(
            &ctx,
            key(EguiKey::M, Modifiers::NONE),
            &claims,
            &scope,
            &mut controller,
            &mut shell,
            &mut waveform,
        );
        assert_eq!(
            controller.markers().map(TrackMarkers::count),
            Some(count_before_m + 1),
            "M must add exactly one point marker"
        );
    }

    // -- Cues: Shift+1 (physical-fallback shape), Shift+2..8
    //    (logical-fallback shape), 1..8 (jump) --
    {
        let (mut controller, _handle, _dirs) = active_controller("inherited-cues");
        controller.queue_replace(vec![track("a", 200_000)]);
        controller.play();
        controller.tick();
        let mut shell = Shell::default();
        let mut waveform = WaveformState::default();
        let ctx = Context::default();
        let claims = FocusClaims::default();
        let scope = now_playing_scope();

        // `Shift+1` (physical-fallback shape, research R3): logical
        // `Exclamationmark` over physical `Num1` — dispatch's pass 1
        // (shift dropped, since physical != logical) must miss, falling
        // through to pass 2 (the physical key with full modifiers).
        let _ = controller.backend_mut().render_buffers(5);
        let cue1_pos = controller.shared().position_frames();
        let ev = key_physical(EguiKey::Exclamationmark, EguiKey::Num1, Modifiers::SHIFT);
        frame(
            &ctx,
            vec![ev, key_release(EguiKey::Exclamationmark)],
            &claims,
            &scope,
            &mut controller,
            &mut shell,
            &mut waveform,
        );
        let slot1 = CueSlot::new(1).unwrap_or_else(|| unreachable!());
        assert_eq!(
            controller
                .markers()
                .and_then(|m| m.cue(slot1))
                .map(|c| c.position),
            Some(cue1_pos),
            "Shift+1 (physical-fallback shape) must set cue slot 1"
        );

        // `Shift+2..8` (logical-fallback shape): shift stays on the
        // logical digit key directly (pass 1 matches with no fallback).
        let digit_keys = [
            EguiKey::Num2,
            EguiKey::Num3,
            EguiKey::Num4,
            EguiKey::Num5,
            EguiKey::Num6,
            EguiKey::Num7,
            EguiKey::Num8,
        ];
        for (offset, digit) in digit_keys.into_iter().enumerate() {
            let n = u8::try_from(offset + 2).unwrap_or_else(|_| unreachable!());
            let _ = controller.backend_mut().render_buffers(5);
            let pos = controller.shared().position_frames();
            press(
                &ctx,
                key(digit, Modifiers::SHIFT),
                &claims,
                &scope,
                &mut controller,
                &mut shell,
                &mut waveform,
            );
            let slot = CueSlot::new(n).unwrap_or_else(|| unreachable!());
            assert_eq!(
                controller
                    .markers()
                    .and_then(|m| m.cue(slot))
                    .map(|c| c.position),
                Some(pos),
                "Shift+{n} must set cue slot {n}"
            );
        }

        // `1` jumps back to cue slot 1, exactly (never a refusal, never
        // changes play/pause state).
        let _ = controller.backend_mut().render_buffers(20);
        let moved = controller.shared().position_frames();
        assert!(moved > cue1_pos, "sanity: playhead must have advanced");
        let intent_before = controller.transport_state().intent;
        press(
            &ctx,
            key(EguiKey::Num1, Modifiers::NONE),
            &claims,
            &scope,
            &mut controller,
            &mut shell,
            &mut waveform,
        );
        controller.tick();
        let _ = controller.backend_mut().render_buffers(1);
        let landed = controller.shared().position_frames();
        assert!(
            landed >= cue1_pos && landed <= cue1_pos + JUMP_LAND_TOLERANCE_FRAMES,
            "1 must jump to (very near) cue slot 1: landed={landed}, cue1_pos={cue1_pos}"
        );
        assert_eq!(controller.transport_state().intent, intent_before);
    }

    // -- Marker-focus nudge arrows: Left/Right/Shift+Left/Shift+Right --
    {
        let (mut controller, _handle, _dirs) = active_controller("inherited-nudge");
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

        let mut shell = Shell::default();
        let mut waveform = WaveformState {
            focused_marker: Some(id),
            ..Default::default()
        };
        let ctx = Context::default();
        let claims = FocusClaims::default();
        let scope = marker_focused_scope();

        let rate = controller.source_sample_rate().max(1);
        let step = (25u64 * u64::from(rate)) / 1000;

        press(
            &ctx,
            key(EguiKey::ArrowRight, Modifiers::NONE),
            &claims,
            &scope,
            &mut controller,
            &mut shell,
            &mut waveform,
        );
        let after_right = controller
            .markers()
            .and_then(|m| m.position_of(id))
            .unwrap_or_else(|| unreachable!());
        assert_eq!(
            after_right,
            start + step,
            "Right must nudge later by one step"
        );

        press(
            &ctx,
            key(EguiKey::ArrowLeft, Modifiers::NONE),
            &claims,
            &scope,
            &mut controller,
            &mut shell,
            &mut waveform,
        );
        let after_left = controller
            .markers()
            .and_then(|m| m.position_of(id))
            .unwrap_or_else(|| unreachable!());
        assert_eq!(after_left, start, "Left must nudge earlier by one step");

        press(
            &ctx,
            key(EguiKey::ArrowRight, Modifiers::SHIFT),
            &claims,
            &scope,
            &mut controller,
            &mut shell,
            &mut waveform,
        );
        let after_shift_right = controller
            .markers()
            .and_then(|m| m.position_of(id))
            .unwrap_or_else(|| unreachable!());
        assert_eq!(
            after_shift_right,
            start + step * 10,
            "Shift+Right must nudge later by 10x the step"
        );

        press(
            &ctx,
            key(EguiKey::ArrowLeft, Modifiers::SHIFT),
            &claims,
            &scope,
            &mut controller,
            &mut shell,
            &mut waveform,
        );
        let after_shift_left = controller
            .markers()
            .and_then(|m| m.position_of(id))
            .unwrap_or_else(|| unreachable!());
        assert_eq!(
            after_shift_left, start,
            "Shift+Left must nudge earlier by 10x the step"
        );
    }
}

// -- T046: the sole FR-017 deviation ----------------------------------------

/// `/` inside a focused text field types the character and does not
/// focus search — the one documented behaviour change from 004/006
/// (FR-017, contracts/ui-actions.md §2 rule 1).
#[test]
fn slash_in_focused_text_field_types_and_does_not_focus_search() {
    let (mut controller, _handle, _dirs) = active_controller("slash-text-field");
    controller.queue_replace(vec![track("a", 200_000)]);

    let mut shell = Shell::default();
    let mut waveform = WaveformState::default();
    let ctx = Context::default();
    let text_field_id = Id::new("test-text-field");
    ctx.memory_mut(|m| m.request_focus(text_field_id));
    let mut claims = FocusClaims::default();
    claims.register(text_field_id, Claim::TextLike);
    let scope = app_scope();

    let invocations = press(
        &ctx,
        key(EguiKey::Slash, Modifiers::NONE),
        &claims,
        &scope,
        &mut controller,
        &mut shell,
        &mut waveform,
    );

    assert!(
        invocations.is_empty(),
        "a focused text-like field must silence every action, including Slash"
    );
    assert_eq!(
        shell.section,
        Section::Library,
        "/ must not jump to Search while a text field is focused"
    );
    assert!(!shell.focus_search_requested);
}

// -- T048: transport, app-wide regardless of section (FR-018, SC-009) ------

/// Every transport shortcut (`Scope::App`) produces the same effect from
/// Now Playing and from Library — SC-009's "identical" claim (US1 AS8).
#[test]
fn transport_shortcuts_from_now_playing_and_library() {
    for (label, scope) in [
        ("now-playing", now_playing_scope()),
        ("library", app_scope()),
    ] {
        let (mut controller, _handle, _dirs) = active_controller(&format!("transport-{label}"));
        controller.queue_replace(vec![track("a", 200_000), track("b", 200_000)]);
        controller.play();
        controller.tick();

        let mut shell = Shell::default();
        let mut waveform = WaveformState::default();
        let ctx = Context::default();
        let claims = FocusClaims::default();

        assert_eq!(controller.transport_state().intent, Intent::Playing);
        press(
            &ctx,
            key(EguiKey::Space, Modifiers::NONE),
            &claims,
            &scope,
            &mut controller,
            &mut shell,
            &mut waveform,
        );
        controller.tick();
        assert_eq!(
            controller.transport_state().intent,
            Intent::Paused,
            "{label}: Space must pause while playing"
        );

        press(
            &ctx,
            key(EguiKey::Space, Modifiers::NONE),
            &claims,
            &scope,
            &mut controller,
            &mut shell,
            &mut waveform,
        );
        controller.tick();
        assert_eq!(
            controller.transport_state().intent,
            Intent::Playing,
            "{label}: Space must resume while paused"
        );

        press(
            &ctx,
            key(EguiKey::Space, Modifiers::SHIFT),
            &claims,
            &scope,
            &mut controller,
            &mut shell,
            &mut waveform,
        );
        controller.tick();
        assert_eq!(
            controller.transport_state().intent,
            Intent::Stopped,
            "{label}: Shift+Space must stop"
        );

        controller.play();
        controller.tick();

        let first_id = controller.current_track().map(|item| item.track.id.clone());
        press(
            &ctx,
            key(EguiKey::ArrowRight, Modifiers::COMMAND),
            &claims,
            &scope,
            &mut controller,
            &mut shell,
            &mut waveform,
        );
        let second_id = controller.current_track().map(|item| item.track.id.clone());
        assert!(
            second_id.is_some() && second_id != first_id,
            "{label}: Primary+Right must skip to the next track"
        );

        press(
            &ctx,
            key(EguiKey::ArrowLeft, Modifiers::COMMAND),
            &claims,
            &scope,
            &mut controller,
            &mut shell,
            &mut waveform,
        );
        let back_id = controller.current_track().map(|item| item.track.id.clone());
        assert_eq!(
            back_id, first_id,
            "{label}: Primary+Left must skip back to the previous track"
        );

        let before_forward = controller.position();
        press(
            &ctx,
            key(EguiKey::ArrowRight, Modifiers::COMMAND | Modifiers::SHIFT),
            &claims,
            &scope,
            &mut controller,
            &mut shell,
            &mut waveform,
        );
        controller.tick();
        let _ = controller.backend_mut().render_buffers(1);
        assert!(
            controller.position() > before_forward,
            "{label}: Primary+Shift+Right must seek forward"
        );

        let before_backward = controller.position();
        press(
            &ctx,
            key(EguiKey::ArrowLeft, Modifiers::COMMAND | Modifiers::SHIFT),
            &claims,
            &scope,
            &mut controller,
            &mut shell,
            &mut waveform,
        );
        controller.tick();
        let _ = controller.backend_mut().render_buffers(1);
        assert!(
            controller.position() < before_backward,
            "{label}: Primary+Shift+Left must seek backward"
        );

        let volume_before = controller.master_volume().value();
        press(
            &ctx,
            key(EguiKey::ArrowUp, Modifiers::COMMAND),
            &claims,
            &scope,
            &mut controller,
            &mut shell,
            &mut waveform,
        );
        assert_eq!(
            controller.master_volume().value(),
            volume_before + 5,
            "{label}: Primary+Up must raise volume by 5"
        );

        press(
            &ctx,
            key(EguiKey::ArrowDown, Modifiers::COMMAND),
            &claims,
            &scope,
            &mut controller,
            &mut shell,
            &mut waveform,
        );
        assert_eq!(
            controller.master_volume().value(),
            volume_before,
            "{label}: Primary+Down must lower volume back by 5"
        );
    }
}

// -- T049: the whole catalog is keyboard-reachable (SC-001, SC-002) --------

/// Every enabled-by-default action's shipped default binding(s) resolve
/// back to it (SC-001's "every host function keyboard-operable"),
/// evaluated at the registry level (the same one `dispatch` consults).
#[test]
fn every_enabled_default_binding_dispatches() {
    let (controller, _handle, _dirs) = active_controller("every-default-binding");
    let scope = marker_focused_scope();
    for action in HostAction::ALL {
        let def = modplayer_core::actions::def(action);
        if !def.enabled_by_default {
            continue;
        }
        for binding in def.default_bindings {
            let chord =
                Chord::parse(binding).unwrap_or_else(|e| panic!("{binding:?} must parse: {e:?}"));
            assert_eq!(
                controller.actions().resolve(chord, &scope),
                Some(ActionId::Host(action)),
                "{action:?}'s default binding {binding:?} must resolve back to it"
            );
        }
    }
}

/// `I`, `O`, `L` arm the loop with zero pointer input (US1 AS7, SC-002) —
/// re-proves `markers.rs`'s own coverage through this file's bare-`Context`
/// harness rather than a rendered `now_playing::show`.
#[test]
fn i_o_l_arms_loop_with_no_pointer() {
    let (mut controller, _handle, _dirs) = active_controller("i-o-l-no-pointer");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();

    let mut shell = Shell::default();
    let mut waveform = WaveformState::default();
    let ctx = Context::default();
    let claims = FocusClaims::default();
    let scope = now_playing_scope();

    press(
        &ctx,
        key(EguiKey::I, Modifiers::NONE),
        &claims,
        &scope,
        &mut controller,
        &mut shell,
        &mut waveform,
    );
    let _ = controller.backend_mut().render_buffers(5);
    press(
        &ctx,
        key(EguiKey::O, Modifiers::NONE),
        &claims,
        &scope,
        &mut controller,
        &mut shell,
        &mut waveform,
    );

    let region = controller
        .markers()
        .and_then(TrackMarkers::current_region)
        .expect("I/O must create a region");
    assert!(
        controller
            .markers()
            .and_then(|m| m.region(region))
            .is_some_and(|r| r.is_complete()),
        "the region must be complete after I/O"
    );

    let _ = controller.backend_mut().render_buffers(1);
    press(
        &ctx,
        key(EguiKey::L, Modifiers::NONE),
        &claims,
        &scope,
        &mut controller,
        &mut shell,
        &mut waveform,
    );
    let _ = controller.backend_mut().render_buffers(1);
    assert_ne!(
        controller.loop_status().state,
        LoopState::Disarmed,
        "L must arm the loop with no pointer input"
    );
}

// -- T050: FR-019 precedence — one consumer per press (SC-010) -------------

/// A focused widget that registered no claim of its own still keeps
/// `Space` via `TOOLKIT_DEFAULT_CLAIMS` (contracts/ui-actions.md §2),
/// exactly like a focused "Stop" button: the dispatcher must produce no
/// invocation, so the widget's own click handler is the single effect
/// (SC-010, "exactly one effect").
#[test]
fn focused_stop_button_wins_space_exactly_one_effect() {
    let (mut controller, _handle, _dirs) = active_controller("focused-stop-wins");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    let intent_before = controller.transport_state().intent;

    let mut shell = Shell::default();
    let mut waveform = WaveformState::default();
    let ctx = Context::default();
    let stop_button_id = Id::new("test-stop-button");
    ctx.memory_mut(|m| m.request_focus(stop_button_id));
    let claims = FocusClaims::default(); // no explicit claim: toolkit default applies
    let scope = now_playing_scope();

    let invocations = press(
        &ctx,
        key(EguiKey::Space, Modifiers::NONE),
        &claims,
        &scope,
        &mut controller,
        &mut shell,
        &mut waveform,
    );

    assert!(
        invocations.is_empty(),
        "a focused unclaimed widget must own Space via the toolkit default"
    );
    assert_eq!(
        controller.transport_state().intent,
        intent_before,
        "play/pause must not also fire while a focused widget owns Space"
    );
}

/// A focused waveform's claim set deliberately excludes `Space`
/// (contracts/ui-actions.md §2), so it reaches the dispatcher and toggles
/// play/pause exactly as if nothing were focused (US1 AS9).
#[test]
fn focused_waveform_lets_space_toggle() {
    let (mut controller, _handle, _dirs) = active_controller("focused-waveform-space");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    assert_eq!(controller.transport_state().intent, Intent::Playing);

    let mut shell = Shell::default();
    let mut waveform = WaveformState::default();
    let ctx = Context::default();
    let waveform_id = Id::new("test-waveform-overview");
    ctx.memory_mut(|m| m.request_focus(waveform_id));
    let mut claims = FocusClaims::default();
    claims.register(waveform_id, Claim::Keys(actions::waveform_claims()));
    let scope = now_playing_scope();

    let invocations = press(
        &ctx,
        key(EguiKey::Space, Modifiers::NONE),
        &claims,
        &scope,
        &mut controller,
        &mut shell,
        &mut waveform,
    );

    assert_eq!(
        invocations.len(),
        1,
        "Space must reach the dispatcher when the waveform (not the toolkit default) is focused"
    );
    assert_eq!(
        controller.transport_state().intent,
        Intent::Paused,
        "Space must toggle play/pause while the waveform is focused"
    );
}

/// A focused volume slider's claim set keeps plain `Left`/`Right` for its
/// own fine adjustment (contracts/ui-actions.md §2) even while a marker
/// is focused (which would otherwise make them resolve to the nudge
/// actions) — the claim wins before the registry is ever consulted.
#[test]
fn focused_volume_slider_keeps_arrows() {
    let (mut controller, _handle, _dirs) = active_controller("focused-volume-arrows");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();

    let id = controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));
    let start = controller
        .markers()
        .and_then(|m| m.position_of(id))
        .unwrap_or_else(|| unreachable!());

    let mut shell = Shell::default();
    let mut waveform = WaveformState {
        focused_marker: Some(id),
        ..Default::default()
    };
    let ctx = Context::default();
    let slider_id = Id::new("test-volume-slider");
    ctx.memory_mut(|m| m.request_focus(slider_id));
    let mut claims = FocusClaims::default();
    claims.register(slider_id, Claim::Keys(actions::volume_slider_claims()));
    let scope = marker_focused_scope();

    let invocations = press(
        &ctx,
        key(EguiKey::ArrowLeft, Modifiers::NONE),
        &claims,
        &scope,
        &mut controller,
        &mut shell,
        &mut waveform,
    );

    assert!(
        invocations.is_empty(),
        "the focused volume slider's claim must keep plain Left for itself"
    );
    assert_eq!(
        controller.markers().and_then(|m| m.position_of(id)),
        Some(start),
        "no nudge must happen while the volume slider owns Left/Right"
    );
}

/// A focused text-like field silences every default binding in the
/// catalog at once (FR-019 rule 1) — the general case behind T046's
/// single-key `Slash` regression.
#[test]
fn text_field_focus_silences_every_action() {
    let (mut controller, _handle, _dirs) = active_controller("text-field-silences-all");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    let marker_id = controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));

    let mut shell = Shell::default();
    let mut waveform = WaveformState {
        focused_marker: Some(marker_id),
        ..Default::default()
    };
    let ctx = Context::default();
    let text_field_id = Id::new("test-text-field-all-actions");
    ctx.memory_mut(|m| m.request_focus(text_field_id));
    let mut claims = FocusClaims::default();
    claims.register(text_field_id, Claim::TextLike);
    let scope = marker_focused_scope();

    let mut events = Vec::new();
    for action in HostAction::ALL {
        for binding in modplayer_core::actions::def(action).default_bindings {
            let chord =
                Chord::parse(binding).unwrap_or_else(|e| panic!("{binding:?} must parse: {e:?}"));
            events.push(key(egui_key(chord.key), egui_mods(chord.mods)));
        }
    }
    assert!(!events.is_empty(), "sanity: the catalog has bound defaults");

    let invocations = frame(
        &ctx,
        events,
        &claims,
        &scope,
        &mut controller,
        &mut shell,
        &mut waveform,
    );
    assert!(
        invocations.is_empty(),
        "a focused text-like field must silence every default binding in the catalog"
    );
}

// -- T051: FR-019's repeat rule ---------------------------------------------

/// A held key produces exactly one invocation for a `repeats_while_held:
/// false` action (`TogglePlayPause`) and one *per event* for a `true` one
/// (`VolumeUp`) — US1 AS10. Deliberately sends repeated `pressed: true`
/// events for the same key with no release in between so egui's own
/// `InputState::begin_pass` marks the second and later ones `repeat: true`
/// itself (it ignores whatever `repeat` an integration sends).
#[test]
fn held_space_toggles_once_held_volume_repeats() {
    let (mut controller, _handle, _dirs) = active_controller("held-repeat");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    assert_eq!(controller.transport_state().intent, Intent::Playing);

    let mut shell = Shell::default();
    let mut waveform = WaveformState::default();
    let ctx = Context::default();
    let claims = FocusClaims::default();
    let scope = now_playing_scope();

    let held_space = vec![
        key(EguiKey::Space, Modifiers::NONE),
        key(EguiKey::Space, Modifiers::NONE),
        key(EguiKey::Space, Modifiers::NONE),
        key(EguiKey::Space, Modifiers::NONE),
    ];
    let invocations = frame(
        &ctx,
        held_space,
        &claims,
        &scope,
        &mut controller,
        &mut shell,
        &mut waveform,
    );
    assert_eq!(
        invocations.len(),
        1,
        "TogglePlayPause does not repeat_while_held: only the initial press must fire"
    );
    assert_eq!(controller.transport_state().intent, Intent::Paused);

    let volume_before = controller.master_volume().value();
    let held_volume_up = vec![
        key(EguiKey::ArrowUp, Modifiers::COMMAND),
        key(EguiKey::ArrowUp, Modifiers::COMMAND),
        key(EguiKey::ArrowUp, Modifiers::COMMAND),
    ];
    let invocations = frame(
        &ctx,
        held_volume_up,
        &claims,
        &scope,
        &mut controller,
        &mut shell,
        &mut waveform,
    );
    assert_eq!(
        invocations.len(),
        3,
        "VolumeUp repeats_while_held: every event must fire"
    );
    assert_eq!(controller.master_volume().value(), volume_before + 15);
}

// -- T052: US1 AS11 -----------------------------------------------------------

/// `Q` toggles the Queue panel only while Now Playing is shown
/// (`Scope::NowPlaying`) — US1 AS11.
#[test]
fn q_toggles_queue_panel_in_now_playing_only() {
    let (mut controller, _handle, _dirs) = active_controller("q-queue-toggle");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();

    let mut shell = Shell::default();
    let mut waveform = WaveformState::default();
    let ctx = Context::default();
    let claims = FocusClaims::default();
    let queue_panel_id = Id::new("now-playing-queue-panel-open");
    let queue_open = || {
        ctx.memory(|m| m.data.get_temp::<bool>(queue_panel_id))
            .unwrap_or(false)
    };

    assert!(!queue_open(), "sanity: the queue panel starts closed");

    // Outside Now Playing: `Q` is bound but `Scope::NowPlaying` isn't
    // live, so it must not resolve at all.
    let invocations = press(
        &ctx,
        key(EguiKey::Q, Modifiers::NONE),
        &claims,
        &app_scope(),
        &mut controller,
        &mut shell,
        &mut waveform,
    );
    assert!(
        invocations.is_empty(),
        "Q must not fire outside Now Playing"
    );
    assert!(!queue_open());

    // In Now Playing: opens, then closes.
    press(
        &ctx,
        key(EguiKey::Q, Modifiers::NONE),
        &claims,
        &now_playing_scope(),
        &mut controller,
        &mut shell,
        &mut waveform,
    );
    assert!(queue_open(), "Q must open the queue panel in Now Playing");

    press(
        &ctx,
        key(EguiKey::Q, Modifiers::NONE),
        &claims,
        &now_playing_scope(),
        &mut controller,
        &mut shell,
        &mut waveform,
    );
    assert!(!queue_open(), "a second Q must close it again");
}

/// `E` toggles the Effect Chain panel only while Now Playing is shown
/// (008, contracts/ui-effect-chain.md §5, FR-017).
#[test]
fn toggle_effect_chain_dispatches_in_now_playing_only() {
    let (mut controller, _handle, _dirs) = active_controller("e-effects-toggle");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();

    let mut shell = Shell::default();
    let mut waveform = WaveformState::default();
    let ctx = Context::default();
    let claims = FocusClaims::default();
    let effects_panel_id = modplayer_ui::effects_view::panel_open_id();
    let effects_open = || {
        ctx.memory(|m| m.data.get_temp::<bool>(effects_panel_id))
            .unwrap_or(false)
    };

    assert!(
        !effects_open(),
        "sanity: the effect chain panel starts closed"
    );

    // Outside Now Playing: `E` is bound but `Scope::NowPlaying` isn't
    // live, so it must not resolve at all.
    let invocations = press(
        &ctx,
        key(EguiKey::E, Modifiers::NONE),
        &claims,
        &app_scope(),
        &mut controller,
        &mut shell,
        &mut waveform,
    );
    assert!(
        invocations.is_empty(),
        "E must not fire outside Now Playing"
    );
    assert!(!effects_open());

    // In Now Playing: opens, then closes.
    press(
        &ctx,
        key(EguiKey::E, Modifiers::NONE),
        &claims,
        &now_playing_scope(),
        &mut controller,
        &mut shell,
        &mut waveform,
    );
    assert!(
        effects_open(),
        "E must open the effect chain panel in Now Playing"
    );

    press(
        &ctx,
        key(EguiKey::E, Modifiers::NONE),
        &claims,
        &now_playing_scope(),
        &mut controller,
        &mut shell,
        &mut waveform,
    );
    assert!(!effects_open(), "a second E must close it again");
}

// -- T053: FR-018 scopes -----------------------------------------------------

/// `I` (`Scope::NowPlaying`) falls through untouched while Now Playing is
/// not shown — the dispatcher must not consume the event or touch the
/// model.
#[test]
fn i_outside_now_playing_falls_through() {
    let (mut controller, _handle, _dirs) = active_controller("i-outside-now-playing");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();

    let mut shell = Shell::default();
    let mut waveform = WaveformState::default();
    let ctx = Context::default();
    let claims = FocusClaims::default();

    let invocations = press(
        &ctx,
        key(EguiKey::I, Modifiers::NONE),
        &claims,
        &app_scope(),
        &mut controller,
        &mut shell,
        &mut waveform,
    );

    assert!(
        invocations.is_empty(),
        "I must not resolve outside Now Playing"
    );
    assert_eq!(
        controller.markers().and_then(TrackMarkers::current_region),
        None,
        "no region must be created"
    );
}

/// The nudge actions (`Scope::MarkerFocused`) never fire while no marker
/// is focused, even from within Now Playing.
#[test]
fn nudge_requires_focused_marker() {
    let (mut controller, _handle, _dirs) = active_controller("nudge-requires-focus");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    let id = controller
        .add_point_marker()
        .unwrap_or_else(|e| unreachable!("add_point_marker: {e}"));
    let start = controller
        .markers()
        .and_then(|m| m.position_of(id))
        .unwrap_or_else(|| unreachable!());

    let mut shell = Shell::default();
    // Deliberately no `focused_marker`.
    let mut waveform = WaveformState::default();
    let ctx = Context::default();
    let claims = FocusClaims::default();

    let invocations = press(
        &ctx,
        key(EguiKey::ArrowRight, Modifiers::NONE),
        &claims,
        &now_playing_scope(),
        &mut controller,
        &mut shell,
        &mut waveform,
    );

    assert!(
        invocations.is_empty(),
        "the nudge actions must not resolve while marker_focused is false"
    );
    assert_eq!(
        controller.markers().and_then(|m| m.position_of(id)),
        Some(start)
    );
}

// -- T054: same-frame dispatch (FR-005, SC-011) + the R2 bijection ---------

/// `dispatch`+`invoke`'s effect is visible immediately after the single
/// frame that carried the key event — no extra `tick()` needed (FR-005,
/// SC-011).
#[test]
fn invocation_lands_in_the_same_frame_as_the_key_event() {
    let (mut controller, _handle, _dirs) = active_controller("same-frame");
    controller.queue_replace(vec![track("a", 200_000), track("b", 200_000)]);
    controller.play();
    controller.tick();
    let first_id = controller.current_track().map(|item| item.track.id.clone());

    let mut shell = Shell::default();
    let mut waveform = WaveformState::default();
    let ctx = Context::default();
    let claims = FocusClaims::default();

    press(
        &ctx,
        key(EguiKey::ArrowRight, Modifiers::COMMAND),
        &claims,
        &now_playing_scope(),
        &mut controller,
        &mut shell,
        &mut waveform,
    );

    let second_id = controller.current_track().map(|item| item.track.id.clone());
    assert!(
        second_id.is_some() && second_id != first_id,
        "NextTrack's effect must already be visible after the one frame that dispatched it"
    );
}

/// `modplayer_core::actions::KEY_NAMES` is exactly `egui::Key::ALL`'s
/// `name()` vocabulary (research R2) — the egui-side half of the
/// bijection `modplayer-core/tests/actions.rs::key_name_table_matches_
/// egui_key_all`'s core-side note (T018) defers to this file for.
#[test]
fn key_name_table_matches_egui_key_all() {
    let egui_names: HashSet<&'static str> = EguiKey::ALL.iter().map(|k| k.name()).collect();
    let core_names: HashSet<&'static str> = KEY_NAMES.iter().copied().collect();

    assert_eq!(
        egui_names.len(),
        EguiKey::ALL.len(),
        "sanity: egui::Key::ALL must have no duplicate names"
    );
    assert_eq!(
        core_names.len(),
        KEY_NAMES.len(),
        "sanity: KEY_NAMES must have no duplicates"
    );
    assert_eq!(
        egui_names, core_names,
        "KEY_NAMES must be exactly egui::Key::ALL's name() vocabulary"
    );

    for k in EguiKey::ALL {
        assert!(
            KeyName::parse(k.name()).is_some(),
            "{k:?}'s name {:?} must parse through KeyName::parse",
            k.name()
        );
    }
}

// -- T072/T074: US3 conflict surfacing at the dispatcher (contracts/
// ui-actions.md §2/§3, data-model.md §3 rules G4/G5) -----------------------

/// Rebinding `ToggleLoop` onto `TogglePlayPause`'s own `Space` key flags
/// both and silences both — `dispatch`'s step 3d falls through with no
/// `Invocation` rather than picking a winner (US3 AS1/AS2, rule G4).
/// Removing the offending binding clears the conflict and each action
/// fires again on its own remaining key (US3 AS5/AS6).
#[test]
fn conflicting_chord_fires_neither_until_resolved() {
    let (mut controller, _handle, _dirs) = active_controller("conflicting-chord");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    assert_eq!(controller.transport_state().intent, Intent::Playing);

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

    let mut shell = Shell::default();
    let mut waveform = WaveformState::default();
    let ctx = Context::default();
    let claims = FocusClaims::default();
    let scope = now_playing_scope();

    let invocations = press(
        &ctx,
        key(EguiKey::Space, Modifiers::NONE),
        &claims,
        &scope,
        &mut controller,
        &mut shell,
        &mut waveform,
    );
    assert!(
        invocations.is_empty(),
        "a conflicting chord must fire neither action"
    );
    assert_eq!(
        controller.transport_state().intent,
        Intent::Playing,
        "TogglePlayPause must not have fired while conflicting"
    );

    controller.remove_binding(HostAction::ToggleLoop, space);
    assert!(
        !controller
            .actions()
            .is_conflicting(HostAction::TogglePlayPause, space),
        "removing the offending binding must clear the conflict"
    );

    let invocations = press(
        &ctx,
        key(EguiKey::Space, Modifiers::NONE),
        &claims,
        &scope,
        &mut controller,
        &mut shell,
        &mut waveform,
    );
    assert_eq!(
        invocations,
        vec![Invocation {
            action: ActionId::Host(HostAction::TogglePlayPause),
            repeat: false
        }],
        "TogglePlayPause must fire again once the conflict is resolved"
    );
    assert_eq!(controller.transport_state().intent, Intent::Paused);

    let invocations = press(
        &ctx,
        key(EguiKey::L, Modifiers::NONE),
        &claims,
        &scope,
        &mut controller,
        &mut shell,
        &mut waveform,
    );
    assert_eq!(
        invocations,
        vec![Invocation {
            action: ActionId::Host(HostAction::ToggleLoop),
            repeat: false
        }],
        "ToggleLoop must still fire on its own remaining key, untouched by the resolved conflict"
    );
}

/// A disabled action's own default bindings never enter the index (rule
/// G2), so they can neither conflict with, nor block, an enabled action
/// that claims the same chord. Enabling the action re-evaluates the
/// conflict set immediately and both actions go silent on that shared
/// chord (rule G5, US3 AS4).
#[test]
fn disabled_tempo_binding_does_not_block_and_flags_on_enable() {
    let (mut controller, _handle, _dirs) = active_controller("disabled-tempo-conflict");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    // 008 flips TempoStepUp's own shipped default to enabled (T049); this
    // test's disabled starting state (needed to exercise the registry's
    // enable/disable mechanics below) is created explicitly instead.
    controller.set_action_enabled(HostAction::TempoStepUp, false);
    assert!(!controller.actions().is_enabled(HostAction::TempoStepUp));

    let equals_name = KeyName::parse("Equals").unwrap_or_else(|| unreachable!());
    let equals = Chord::parse("Equals").unwrap_or_else(|_| unreachable!()); // TempoStepUp's own default.
    controller
        .add_binding(HostAction::NavPlugins, equals)
        .unwrap_or_else(|_| unreachable!());
    assert!(
        !controller
            .actions()
            .is_conflicting(HostAction::NavPlugins, equals),
        "a disabled action's own default binding must not block another action from claiming it"
    );

    let mut shell = Shell::default();
    let mut waveform = WaveformState::default();
    let ctx = Context::default();
    let claims = FocusClaims::default();
    let scope = now_playing_scope();

    let invocations = press(
        &ctx,
        key(egui_key(equals_name), Modifiers::NONE),
        &claims,
        &scope,
        &mut controller,
        &mut shell,
        &mut waveform,
    );
    assert_eq!(
        invocations,
        vec![Invocation {
            action: ActionId::Host(HostAction::NavPlugins),
            repeat: false
        }],
        "a disabled action must never block an enabled one from resolving its shared chord"
    );
    assert_eq!(shell.section, Section::Plugins);

    controller.set_action_enabled(HostAction::TempoStepUp, true);
    assert!(
        controller
            .actions()
            .is_conflicting(HostAction::TempoStepUp, equals)
            && controller
                .actions()
                .is_conflicting(HostAction::NavPlugins, equals),
        "enabling TempoStepUp must flag both actions on their shared chord"
    );

    shell.section = Section::Library;
    let invocations = press(
        &ctx,
        key(egui_key(equals_name), Modifiers::NONE),
        &claims,
        &scope,
        &mut controller,
        &mut shell,
        &mut waveform,
    );
    assert!(
        invocations.is_empty(),
        "the now-conflicting chord must fire neither action"
    );
    assert_eq!(
        shell.section,
        Section::Library,
        "NavPlugins must not have fired while conflicting"
    );
}

// -----------------------------------------------------------------------
// T050 (FR-017, SC-012, US1 AS7/AS8): the tempo-step actions.
// -----------------------------------------------------------------------

/// `TempoStepUp`/`TempoStepDown` ship enabled by default (008 flips T049)
/// and both `repeats_while_held` — a held `Equals` fires once per event,
/// nudging the first time-stretch node's ratio each time.
#[test]
fn tempo_actions_enabled_and_repeat() {
    let (mut controller, _handle, _dirs) =
        active_controller_without_bundled_plugins("tempo-enabled-repeat");
    assert!(controller.actions().is_enabled(HostAction::TempoStepUp));
    assert!(controller.actions().is_enabled(HostAction::TempoStepDown));

    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    let id = controller
        .chain_add_node(NodeKind::TimeStretch)
        .expect("add time stretch");

    let mut shell = Shell::default();
    let mut waveform = WaveformState::default();
    let ctx = Context::default();
    let claims = FocusClaims::default(); // nothing focused: toolkit default owns nothing here
    let scope = now_playing_scope();

    let held_up = vec![
        key(EguiKey::Equals, Modifiers::NONE),
        key(EguiKey::Equals, Modifiers::NONE),
        key(EguiKey::Equals, Modifiers::NONE),
    ];
    let invocations = frame(
        &ctx,
        held_up,
        &claims,
        &scope,
        &mut controller,
        &mut shell,
        &mut waveform,
    );
    assert_eq!(
        invocations.len(),
        3,
        "TempoStepUp repeats_while_held: every event must fire"
    );
    let ratio = controller
        .chain()
        .nodes()
        .iter()
        .find(|n| n.id == id)
        .expect("node")
        .params[0];
    assert!(
        (ratio - 1.30).abs() < 1e-6,
        "three up-steps from the 1.0 default must land on 1.30, got {ratio}"
    );
}

/// `Equals` steps tempo when no widget owns it, but a focused waveform's
/// own `WAVEFORM_CLAIMS` (contracts/ui-actions.md §2) already claims
/// `Plus`/`Equals`/`Minus`, so it keeps its own meaning for them instead of
/// the tempo action ever reaching the dispatcher.
#[test]
fn plus_minus_step_tempo_unless_waveform_focused() {
    let (mut controller, _handle, _dirs) =
        active_controller_without_bundled_plugins("tempo-vs-waveform");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    let id = controller
        .chain_add_node(NodeKind::TimeStretch)
        .expect("add time stretch");

    let mut shell = Shell::default();
    let mut waveform = WaveformState::default();
    let ctx = Context::default();
    let claims = FocusClaims::default();
    let scope = now_playing_scope();

    let invocations = press(
        &ctx,
        key(EguiKey::Equals, Modifiers::NONE),
        &claims,
        &scope,
        &mut controller,
        &mut shell,
        &mut waveform,
    );
    assert_eq!(
        invocations,
        vec![Invocation {
            action: ActionId::Host(HostAction::TempoStepUp),
            repeat: false
        }],
        "Equals must step tempo when no widget owns it"
    );
    let ratio_after_first = controller
        .chain()
        .nodes()
        .iter()
        .find(|n| n.id == id)
        .expect("node")
        .params[0];
    assert!((ratio_after_first - 1.10).abs() < 1e-6);

    let waveform_id = Id::new("test-waveform-overview-tempo");
    ctx.memory_mut(|m| m.request_focus(waveform_id));
    let mut claims = FocusClaims::default();
    claims.register(waveform_id, Claim::Keys(actions::waveform_claims()));

    let invocations = press(
        &ctx,
        key(EguiKey::Equals, Modifiers::NONE),
        &claims,
        &scope,
        &mut controller,
        &mut shell,
        &mut waveform,
    );
    assert!(
        invocations.is_empty(),
        "a focused waveform must own Equals via its own claim, not the tempo action"
    );
    let ratio_after_second = controller
        .chain()
        .nodes()
        .iter()
        .find(|n| n.id == id)
        .expect("node")
        .params[0];
    assert_eq!(
        ratio_after_second, ratio_after_first,
        "the ratio must not change while the waveform owns the key"
    );
}

/// The real-keyboard form of the test above: on a US layout `+` arrives
/// as logical `Plus` + physical `Equals` with `Shift` held. The focused
/// waveform's `plain(Plus)` claim must still own it — the claim check
/// normalises the layout-consumed `Shift` exactly like the registry
/// lookup does — so the tempo never steps (2026-09-19 manual walk, M6,
/// where the raw-modifier match let `Shift`+`=` fall through to
/// `TempoStepUp` with the waveform focused).
#[test]
fn shift_equals_plus_on_focused_waveform_never_steps_tempo() {
    let (mut controller, _handle, _dirs) =
        active_controller_without_bundled_plugins("tempo-vs-waveform-shift-plus");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();
    let id = controller
        .chain_add_node(NodeKind::TimeStretch)
        .expect("add time stretch");

    let mut shell = Shell::default();
    let mut waveform = WaveformState::default();
    let ctx = Context::default();
    let scope = now_playing_scope();

    let waveform_id = Id::new("test-waveform-overview-shift-plus");
    ctx.memory_mut(|m| m.request_focus(waveform_id));
    let mut claims = FocusClaims::default();
    claims.register(waveform_id, Claim::Keys(actions::waveform_claims()));

    let invocations = press(
        &ctx,
        key_physical(EguiKey::Plus, EguiKey::Equals, Modifiers::SHIFT),
        &claims,
        &scope,
        &mut controller,
        &mut shell,
        &mut waveform,
    );
    assert!(
        invocations.is_empty(),
        "Shift+= (a typed `+`) must be owned by the focused waveform, got {invocations:?}"
    );
    let ratio = controller
        .chain()
        .nodes()
        .iter()
        .find(|n| n.id == id)
        .expect("node")
        .params[0];
    assert!(
        (ratio - 1.0).abs() < 1e-6,
        "tempo must stay at 1.0 while the waveform owns `+`, got {ratio}"
    );

    // Without the waveform focused the same event still steps tempo.
    ctx.memory_mut(|m| m.surrender_focus(waveform_id));
    let invocations = press(
        &ctx,
        key_physical(EguiKey::Plus, EguiKey::Equals, Modifiers::SHIFT),
        &FocusClaims::default(),
        &scope,
        &mut controller,
        &mut shell,
        &mut waveform,
    );
    assert_eq!(
        invocations,
        vec![Invocation {
            action: ActionId::Host(HostAction::TempoStepUp),
            repeat: false
        }],
        "unfocused, Shift+= is the tempo step"
    );
}

/// contracts/effects-service.md §2 rule C3: holding `Plus` with no
/// time-stretch node in the chain raises the keyed `effects-no-time-
/// stretch` Info exactly once, even though every held event still
/// produces its own `TempoStepUp` invocation (`repeats_while_held`).
#[test]
fn plus_without_time_stretch_notifies_once_while_held() {
    let (mut controller, _handle, _dirs) = active_controller("tempo-no-node-notify");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();

    let mut shell = Shell::default();
    let mut waveform = WaveformState::default();
    let ctx = Context::default();
    let claims = FocusClaims::default();
    let scope = now_playing_scope();

    let held_plus = vec![
        key(EguiKey::Plus, Modifiers::NONE),
        key(EguiKey::Plus, Modifiers::NONE),
        key(EguiKey::Plus, Modifiers::NONE),
    ];
    let invocations = frame(
        &ctx,
        held_plus,
        &claims,
        &scope,
        &mut controller,
        &mut shell,
        &mut waveform,
    );
    assert_eq!(
        invocations.len(),
        3,
        "TempoStepUp still repeats_while_held with no time-stretch node"
    );

    let visible_count = controller
        .notifications()
        .visible()
        .filter(|n| n.message_key == KEY_EFFECTS_NO_TIME_STRETCH)
        .count();
    assert_eq!(
        visible_count, 1,
        "must raise the no-time-stretch notification exactly once across the held run"
    );
}

// -- 010-transport-focus, Phase 5 (US3): `T` toggles the Transport panel ----

/// `T` toggles the Transport panel only while Now Playing is shown
/// (010-transport-focus, contracts/ui-transport-panel.md §1).
#[test]
fn t_toggles_transport_panel_in_now_playing_scope() {
    let (mut controller, _handle, _dirs) = active_controller("t-transport-toggle");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();

    let mut shell = Shell::default();
    let mut waveform = WaveformState::default();
    let ctx = Context::default();
    let claims = FocusClaims::default();
    let transport_panel_id = modplayer_ui::transport_view::panel_open_id();
    let transport_open = || {
        ctx.memory(|m| m.data.get_temp::<bool>(transport_panel_id))
            .unwrap_or(false)
    };

    assert!(
        !transport_open(),
        "sanity: the Transport panel starts closed"
    );

    // In Now Playing: opens, then closes.
    press(
        &ctx,
        key(EguiKey::T, Modifiers::NONE),
        &claims,
        &now_playing_scope(),
        &mut controller,
        &mut shell,
        &mut waveform,
    );
    assert!(
        transport_open(),
        "T must open the Transport panel in Now Playing"
    );

    press(
        &ctx,
        key(EguiKey::T, Modifiers::NONE),
        &claims,
        &now_playing_scope(),
        &mut controller,
        &mut shell,
        &mut waveform,
    );
    assert!(!transport_open(), "a second T must close it again");
}

/// `T` inside a focused text-like field types the character and never
/// toggles the panel (FR-017's general precedence rule, mirrors `slash_
/// in_focused_text_field_types_and_does_not_focus_search`).
#[test]
fn t_ignored_while_text_field_focused() {
    let (mut controller, _handle, _dirs) = active_controller("t-text-field-focused");
    controller.queue_replace(vec![track("a", 200_000)]);

    let mut shell = Shell::default();
    let mut waveform = WaveformState::default();
    let ctx = Context::default();
    let text_field_id = Id::new("test-text-field");
    ctx.memory_mut(|m| m.request_focus(text_field_id));
    let mut claims = FocusClaims::default();
    claims.register(text_field_id, Claim::TextLike);
    let transport_panel_id = modplayer_ui::transport_view::panel_open_id();

    let invocations = press(
        &ctx,
        key(EguiKey::T, Modifiers::NONE),
        &claims,
        &now_playing_scope(),
        &mut controller,
        &mut shell,
        &mut waveform,
    );

    assert!(
        invocations.is_empty(),
        "a focused text-like field must silence T, like every other action"
    );
    assert!(
        !ctx.memory(|m| m.data.get_temp::<bool>(transport_panel_id))
            .unwrap_or(false),
        "the Transport panel must stay closed"
    );
}

/// `T` (`Scope::NowPlaying`) falls through untouched while Now Playing is
/// not shown — mirrors `i_outside_now_playing_falls_through`.
#[test]
fn t_ignored_outside_now_playing() {
    let (mut controller, _handle, _dirs) = active_controller("t-outside-now-playing");
    controller.queue_replace(vec![track("a", 200_000)]);
    controller.play();
    controller.tick();

    let mut shell = Shell::default();
    let mut waveform = WaveformState::default();
    let ctx = Context::default();
    let claims = FocusClaims::default();

    let invocations = press(
        &ctx,
        key(EguiKey::T, Modifiers::NONE),
        &claims,
        &app_scope(),
        &mut controller,
        &mut shell,
        &mut waveform,
    );
    assert!(
        invocations.is_empty(),
        "T must not fire outside Now Playing"
    );
    assert!(
        !ctx.memory(|m| m
            .data
            .get_temp::<bool>(modplayer_ui::transport_view::panel_open_id()))
            .unwrap_or(false),
        "the Transport panel must stay closed"
    );
}

// -- 011-plugin-ui-contributions US2 (T065, contracts/action-registry-
// plugins.md D1/D2): the dispatcher resolves a registered plugin action's
// own bound chord to `ActionId::Plugin` and invokes it exactly like a
// `HostAction` — mirrors `plugin_panels.rs`'s own fixture harness (this
// file's `active_controller` above never sets `MODPLAYER_PLUGIN_FIXTURES`,
// so a small dedicated harness lives here instead). --------------------

const UI_SHORTCUTS: &str = "org.modplayer.fixture.ui-shortcuts";

/// `MODPLAYER_PLUGIN_FIXTURES`/`MODPLAYER_PLUGIN_STATE_DIR`/
/// `MODPLAYER_TRACK_STATE_DIR` are process-global (mirrors `plugin_panels.
/// rs`'s own `PLUGIN_ENV_LOCK`) — a separate lock from
/// `TRACK_STATE_ENV_LOCK` above since only this section's one test needs
/// the fixture pipeline at all.
static PLUGIN_ENV_LOCK: Mutex<()> = Mutex::new(());

fn plugin_fixture_controller(
    label: &str,
) -> (
    PlaybackController<FakeBackend, ScriptedHost>,
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
            PlaybackController::new(FakeBackend::new(vec![]), ScriptedHost::new(), store);
        unsafe {
            std::env::remove_var("MODPLAYER_PLUGIN_FIXTURES");
            std::env::remove_var("MODPLAYER_PLUGIN_STATE_DIR");
            std::env::remove_var("MODPLAYER_TRACK_STATE_DIR");
        }
        controller
    };
    (controller, dir, plugin_state_dir, track_state_dir)
}

fn plugin_fixture_id(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
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

fn pump_plugin_until(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    timeout: Duration,
    mut done: impl FnMut(&mut PlaybackController<FakeBackend, ScriptedHost>) -> bool,
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

/// `dispatch` resolves a registered plugin action's own (freshly bound,
/// conflict-free) chord to `ActionId::Plugin`, and `invoke` (D2) delivers
/// it through `PlaybackController::invoke_plugin_action` exactly like a
/// `HostAction` reaches its own `invoke` arm.
#[test]
fn dispatch_returns_plugin_action_id() {
    let (mut controller, _dir, _psd, _tsd) = plugin_fixture_controller("dispatch-plugin-action");
    let id = plugin_fixture_id(&mut controller, UI_SHORTCUTS);
    // `plugin_fixture_controller` only discovers records — spawn this one
    // explicitly (mirrors `plugin_panels.rs::launch_ui_panel`).
    let shared = std::sync::Arc::clone(controller.shared());
    controller.plugins_mut().spawn(id, &shared);
    assert!(pump_plugin_until(
        &mut controller,
        Duration::from_secs(2),
        |c| {
            matches!(
                c.plugins_mut().record(id).map(|r| &r.lifecycle),
                Some(Lifecycle::Active)
            )
        }
    ));

    let identifier = controller
        .plugins_mut()
        .record(id)
        .unwrap_or_else(|| unreachable!())
        .identifier
        .clone();
    // `tab_bound` ships with a default `Tab` binding 007's own capture
    // would reject (G10), so it registers unbound — free to bind to a
    // fresh, conflict-free chord here without disturbing any other
    // fixture's own default (contrast `take_over`/`nudge`, deliberately
    // conflicting for M5/M6).
    let action_id =
        PluginActionId::parse(&format!("{identifier}.tab_bound")).unwrap_or_else(|| unreachable!());
    assert!(pump_plugin_until(
        &mut controller,
        Duration::from_secs(2),
        |c| { c.actions().plugin_action_registered(&action_id) }
    ));

    let chord = Chord::parse("F8").unwrap_or_else(|_| unreachable!());
    controller
        .add_binding(action_id.clone(), chord)
        .unwrap_or_else(|_| unreachable!());
    assert!(
        !controller
            .actions()
            .is_conflicting(action_id.clone(), chord),
        "sanity: F8 must be free"
    );

    let mut shell = Shell::default();
    let mut waveform = WaveformState::default();
    let ctx = Context::default();
    let claims = FocusClaims::default();
    let scope = app_scope();

    let invocations = press(
        &ctx,
        key(egui_key(chord.key), egui_mods(chord.mods)),
        &claims,
        &scope,
        &mut controller,
        &mut shell,
        &mut waveform,
    );
    assert_eq!(
        invocations,
        vec![Invocation {
            action: ActionId::Plugin(action_id.clone()),
            repeat: false
        }],
        "dispatch must resolve the plugin action's own bound chord to ActionId::Plugin"
    );

    assert!(
        pump_plugin_until(&mut controller, Duration::from_secs(2), |c| {
            call_probe(c, id, "invoked_tab_bound") == 1
        }),
        "invoke() must deliver action_invoked to the plugin via invoke_plugin_action"
    );
}

/// `Request::DebugProbe` straight to `drain_plugin_requests()`, bypassing
/// the gateway's own admission (mirrors `plugin_panels.rs`'s own
/// `call`/`interaction_count` pair, and `controller_plugin_ui.rs`'s own
/// core-side equivalent). `-1` for any refusal/decode failure — every
/// counter this file probes starts at `0`, so `-1` never spuriously
/// compares equal to an expected count.
fn call_probe(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    plugin: PluginId,
    name: &str,
) -> i64 {
    use modplayer_capability_gateway::request::{Request, Response};
    use modplayer_core::plugins::to_gateway_id;
    use modplayer_plugin_runtime::handle::RpcEnvelope;

    let (reply_tx, reply_rx) = std::sync::mpsc::sync_channel(1);
    let envelope = RpcEnvelope {
        plugin: to_gateway_id(plugin),
        request: Request::DebugProbe {
            name: name.to_string(),
        },
        reply: reply_tx,
    };
    controller
        .plugins_mut()
        .debug_requests_sender()
        .send(envelope)
        .unwrap_or_else(|_| unreachable!("the request channel must accept a synthetic envelope"));
    controller.tick();
    match reply_rx.try_recv() {
        Ok(Ok(Response::Probe(value))) => value.as_i64().unwrap_or(-1),
        _ => -1,
    }
}

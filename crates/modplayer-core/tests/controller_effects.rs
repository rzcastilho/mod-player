// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! contracts/effects-service.md §1 (`ChainModel`, rules G1-G9) and §2
//! (`PlaybackController` façade, rules C1, C2, C4, C9, C10).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use modplayer_audio_io::{FakeBackend, FakeDevice};
use modplayer_audio_source::{Availability, SourceCommand, SourceEvent, TrackId, TrackRef};
use modplayer_audio_source_synthetic::{ScriptedHost, ScriptedHostHandle};
use modplayer_core::notifications::{
    KEY_EFFECT_CHAIN_AUTO_BYPASSED, KEY_EFFECT_CHAIN_OVER_BUDGET, KEY_EFFECTS_NO_TIME_STRETCH,
};
use modplayer_core::settings::SettingsStore;
use modplayer_core::{ChainError, ChainModel, PlaybackController, Severity};
use modplayer_effects::catalog::{self, NodeKind, NodeOwner, ParamId};
use modplayer_engine::{BufferPreset, DeviceId, Event, FrameCount, SampleRate};
use proptest::prelude::*;

// -----------------------------------------------------------------------
// `ChainModel` — pure, contracts/effects-service.md §1 rules G1-G9.
// -----------------------------------------------------------------------

/// G1: `add` beyond `MAX_NODES` refuses without corrupting the model or
/// slot set.
#[test]
fn add_17th_is_refused_without_corruption() {
    let mut model = ChainModel::new(44_100);
    for _ in 0..16 {
        model
            .add(NodeKind::Gain, NodeOwner::Host)
            .expect("first 16 must succeed");
    }
    let before = model.nodes().to_vec();
    let err = model
        .add(NodeKind::Gain, NodeOwner::Host)
        .expect_err("17th must be refused");
    assert_eq!(err, ChainError::Full);
    assert_eq!(model.nodes().to_vec(), before, "model must be unchanged");
}

/// G2: ids are monotonic and never reused; slots are (the lowest free).
#[test]
fn ids_never_reused_slots_are() {
    let mut model = ChainModel::new(44_100);
    let (id_a, _) = model.add(NodeKind::Gain, NodeOwner::Host).expect("add a");
    model.remove(id_a).expect("remove a");
    let (id_b, _) = model.add(NodeKind::Gain, NodeOwner::Host).expect("add b");
    assert_ne!(id_a, id_b, "ids must never be reused");
    assert_eq!(
        model.nodes()[0].slot,
        0,
        "the freed slot must be reused (lowest free)"
    );
}

proptest! {
    /// G3: `set_param` clamps with `catalog::clamp` and stores the
    /// clamped value, for any requested value on any of Gain's params.
    #[test]
    fn set_param_returns_and_stores_clamped(
        param_idx in 0usize..2,
        value in -100_000f32..100_000f32,
    ) {
        let mut model = ChainModel::new(44_100);
        let (id, _) = model.add(NodeKind::Gain, NodeOwner::Host).expect("add");
        let param = catalog::params(NodeKind::Gain)[param_idx].id;
        let (clamped, commands) = model.set_param(id, param, value).expect("set_param");
        let expected = catalog::clamp(NodeKind::Gain, param, value, 44_100);
        prop_assert_eq!(clamped, expected);
        prop_assert!(!commands.is_empty());
        let node = model.nodes().iter().find(|n| n.id == id).expect("node");
        prop_assert_eq!(node.params[param_idx], expected);
    }
}

/// G6: a no-op (`Ok`, no commands) at either end of the chain.
#[test]
fn move_by_at_ends_is_noop() {
    let mut model = ChainModel::new(44_100);
    let (id, _) = model.add(NodeKind::Gain, NodeOwner::Host).expect("add");
    assert!(model.move_by(id, -1).expect("move_by left").is_empty());
    assert!(model.move_by(id, 1).expect("move_by right").is_empty());
}

/// G7: `set_source_rate` re-clamps only `nyquist_clamped` params and
/// emits a command only for a value that actually changed.
#[test]
fn rate_change_reclamps_only_frequencies() {
    let mut model = ChainModel::new(44_100);
    let (id, _) = model
        .add(NodeKind::Filter, NodeOwner::Host)
        .expect("add filter");
    model
        .set_param(id, ParamId(1), 19_000.0)
        .expect("set cutoff");
    let commands = model.set_source_rate(8_000);
    assert!(
        !commands.is_empty(),
        "cutoff must re-clamp once the Nyquist bound drops below it"
    );
    assert!(
        commands
            .iter()
            .all(|c| matches!(c, modplayer_engine::Command::ChainSetParam { param, .. } if *param == ParamId(1))),
        "only the frequency parameter may change"
    );
}

/// G7/FR-014 (Phase 5, T072): `set_source_rate` walks *every* band of an
/// `Equalizer` node, re-clamping each band's `freq` independently and
/// leaving every band's `gain`/`q` untouched — the same rule as
/// `rate_change_reclamps_only_frequencies` above, now proven over the
/// EQ's `16 + 4*band + n` multi-parameter layout rather than a single
/// frequency. The new rate (20 kHz, effective max `0.45 * 20_000 =
/// 9_000` Hz) is chosen so every *other* band's default centre frequency
/// (up to band 7's `8_064` Hz) still fits — only band 1's explicitly
/// out-of-bound request must move.
#[test]
fn rate_change_rebuild_reclamps_eq() {
    let mut model = ChainModel::new(44_100);
    let (id, _) = model
        .add(NodeKind::Equalizer, NodeOwner::Host)
        .expect("add eq");
    model
        .set_param(id, ParamId::eq_band(0, 0), 3_000.0)
        .expect("set band 0 freq");
    model
        .set_param(id, ParamId::eq_band(1, 0), 19_000.0)
        .expect("set band 1 freq");
    model
        .set_param(id, ParamId::eq_band(1, 1), 6.0)
        .expect("set band 1 gain");
    model
        .set_param(id, ParamId::eq_band(1, 2), 4.0)
        .expect("set band 1 q");

    let commands = model.set_source_rate(20_000);

    assert_eq!(
        commands.len(),
        1,
        "only band 1's freq exceeds the new 9_000 Hz Nyquist bound: {commands:?}"
    );
    assert!(matches!(
        commands[0],
        modplayer_engine::Command::ChainSetParam { param, .. }
            if param == ParamId::eq_band(1, 0)
    ));

    let node = model.nodes().iter().find(|n| n.id == id).expect("node");
    let pos = |id: ParamId| {
        catalog::params(NodeKind::Equalizer)
            .iter()
            .position(|p| p.id == id)
            .expect("param exists")
    };
    assert_eq!(
        node.params[pos(ParamId::eq_band(0, 0))],
        3_000.0,
        "band 0's freq was already within bound and must be untouched"
    );
    assert_eq!(
        node.params[pos(ParamId::eq_band(1, 0))],
        9_000.0,
        "band 1's freq must re-clamp to 0.45 * 20_000"
    );
    assert_eq!(
        node.params[pos(ParamId::eq_band(1, 1))],
        6.0,
        "gain is never Nyquist-clamped"
    );
    assert_eq!(
        node.params[pos(ParamId::eq_band(1, 2))],
        4.0,
        "q is never Nyquist-clamped"
    );
}

/// G8: `replay()` reproduces the RT state from scratch — an insert per
/// node in order, plus a param command for every value differing from
/// its default.
#[test]
fn replay_is_complete_and_ordered() {
    let mut model = ChainModel::new(44_100);
    let (id_a, _) = model.add(NodeKind::Gain, NodeOwner::Host).expect("add a");
    model.set_param(id_a, ParamId(0), -6.0).expect("set_param");
    let (_id_b, _) = model.add(NodeKind::Gain, NodeOwner::Host).expect("add b");

    let commands = model.replay();
    let inserts: Vec<_> = commands
        .iter()
        .filter(|c| matches!(c, modplayer_engine::Command::ChainInsert { .. }))
        .collect();
    assert_eq!(inserts.len(), 2, "one insert per node");
    assert!(matches!(
        commands[0],
        modplayer_engine::Command::ChainInsert { position: 0, .. }
    ));
    let params = commands
        .iter()
        .filter(|c| matches!(c, modplayer_engine::Command::ChainSetParam { .. }))
        .count();
    assert_eq!(params, 1, "only the non-default level must be replayed");
}

/// G9: `first_time_stretch` ignores bypass and returns the earliest in
/// order.
#[test]
fn first_time_stretch_ignores_bypass() {
    let mut model = ChainModel::new(44_100);
    let (id, _) = model
        .add(NodeKind::TimeStretch, NodeOwner::Host)
        .expect("add");
    model.set_bypass(id, true).expect("set_bypass");
    assert_eq!(model.first_time_stretch(), Some(id));
}

/// G4/FR-008: a `Performance`-mode node auto-switches to `Quality` the
/// moment its value leaves stage-use range, and auto-reverts to
/// `Performance` once it is back in range — but only because the
/// controller (not the user) put it there.
#[test]
fn auto_switch_flags_and_reverts() {
    let mut model = ChainModel::new(44_100);
    let (id, _) = model
        .add(NodeKind::PitchShift, NodeOwner::Host)
        .expect("add");

    // |semitones| > 3 leaves stage-use range: auto-switch to Quality.
    let (_, commands) = model.set_param(id, ParamId(0), 7.0).expect("set_param");
    assert_eq!(commands.len(), 2, "value + mode commands");
    assert!(matches!(
        commands[1],
        modplayer_engine::Command::ChainSetParam { param, value, .. }
            if param == ParamId(2) && value == catalog::QualityMode::Quality as u8 as f32
    ));
    let node = model.nodes().iter().find(|n| n.id == id).expect("node");
    let state = node.mode_state.expect("mode_state");
    assert_eq!(state.mode, catalog::QualityMode::Quality);
    assert!(state.auto_switched);

    // Back within |semitones| <= 3: auto-reverts to Performance, because
    // the flag says the controller (not the user) chose Quality.
    let (_, commands) = model.set_param(id, ParamId(0), 2.0).expect("set_param");
    assert_eq!(commands.len(), 2, "value + mode commands");
    assert!(matches!(
        commands[1],
        modplayer_engine::Command::ChainSetParam { param, value, .. }
            if param == ParamId(2) && value == catalog::QualityMode::Performance as u8 as f32
    ));
    let node = model.nodes().iter().find(|n| n.id == id).expect("node");
    let state = node.mode_state.expect("mode_state");
    assert_eq!(state.mode, catalog::QualityMode::Performance);
    assert!(!state.auto_switched);
}

/// G4/FR-008 rule 3: an explicit user `Quality` choice persists through a
/// later excursion back into stage-use range — unlike an auto-switched
/// one, it never auto-reverts.
#[test]
fn user_forced_performance_persists_until_next_excursion() {
    let mut model = ChainModel::new(44_100);
    let (id, _) = model
        .add(NodeKind::TimeStretch, NodeOwner::Host)
        .expect("add");

    // Explicit user choice: Quality, while still in stage-use range
    // (ratio default 1.0).
    model
        .set_mode(id, catalog::QualityMode::Quality)
        .expect("set_mode");
    let node = model.nodes().iter().find(|n| n.id == id).expect("node");
    let state = node.mode_state.expect("mode_state");
    assert_eq!(state.mode, catalog::QualityMode::Quality);
    assert!(!state.auto_switched, "a user choice is never auto-switched");

    // Still in stage-use range ([0.5, 1.5]): rule 2 (auto-revert) never
    // applies to a non-auto-switched Quality node, so it must persist.
    let (_, commands) = model.set_param(id, ParamId(0), 1.2).expect("set_param");
    assert!(
        commands
            .iter()
            .all(|c| !matches!(c, modplayer_engine::Command::ChainSetParam { param, .. } if *param == ParamId(1))),
        "no mode command must be emitted: the user's Quality choice persists"
    );
    let node = model.nodes().iter().find(|n| n.id == id).expect("node");
    let state = node.mode_state.expect("mode_state");
    assert_eq!(state.mode, catalog::QualityMode::Quality);
    assert!(!state.auto_switched);

    // Only the *next excursion* out of stage-use range re-evaluates the
    // rule from scratch (rule 1: Performance -> Quality on excursion —
    // a no-op here since it is already Quality, but this proves the state
    // machine still runs rather than being frozen).
    let (_, commands) = model.set_param(id, ParamId(0), 1.8).expect("set_param");
    assert!(
        commands
            .iter()
            .all(|c| !matches!(c, modplayer_engine::Command::ChainSetParam { param, .. } if *param == ParamId(1))),
        "already Quality: an excursion emits no redundant mode command"
    );
}

// -----------------------------------------------------------------------
// `PlaybackController` façade — contracts/effects-service.md §2 rules
// C1, C2, C4, C9, C10.
// -----------------------------------------------------------------------

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-controller-effects-{}-{}",
            std::process::id(),
            unique
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

fn fresh_store() -> (SettingsStore, TempDir) {
    let dir = TempDir::new();
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

fn track(id: &str) -> TrackRef {
    TrackRef::new(
        TrackId::new(format!("spotify:track:{id}")).unwrap_or_else(|_| unreachable!()),
        id,
        vec!["Artist".to_string()],
        None,
        None,
        180_000,
        Availability::Available,
    )
}

/// Build a controller over a confirmed device, ready for `play()` (mirrors
/// `controller_streaming.rs`'s `ready_controller`).
fn ready_controller() -> (PlaybackController<FakeBackend, ScriptedHost>, TempDir) {
    let (store, dir) = fresh_store();
    let host = ScriptedHost::new();
    let devices = vec![fake_device()];
    let mut controller = PlaybackController::new(FakeBackend::new(devices), host, store);
    controller.launch();
    let dev_id = DeviceId::new("dev-1").unwrap_or_else(|| unreachable!());
    controller.confirm_device(dev_id, BufferPreset::Balanced);
    (controller, dir)
}

/// `ready_controller`, but also returning the `ScriptedHostHandle` (research
/// R8's tests script `SourceEvent::EndOfTrack` directly and inspect
/// `record_commands()` for the drift-bounded `SourceCommand::Seek`).
fn ready_controller_with_handle() -> (
    PlaybackController<FakeBackend, ScriptedHost>,
    ScriptedHostHandle,
    TempDir,
) {
    let (store, dir) = fresh_store();
    let host = ScriptedHost::new();
    let handle = host.handle();
    let devices = vec![fake_device()];
    let mut controller = PlaybackController::new(FakeBackend::new(devices), host, store);
    controller.launch();
    let dev_id = DeviceId::new("dev-1").unwrap_or_else(|| unreachable!());
    controller.confirm_device(dev_id, BufferPreset::Balanced);
    (controller, handle, dir)
}

fn track_with_duration(id: &str, duration_ms: u32) -> TrackRef {
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

/// C1: every façade call applies the model op and pushes its commands —
/// proven here by the model updating in exactly the call order (each call
/// returning `Ok`, so nothing was silently dropped).
#[test]
fn commands_are_pushed_in_model_order() {
    let (mut controller, _dir) = ready_controller();
    let a = controller.chain_add_node(NodeKind::Gain).expect("add a");
    let b = controller.chain_add_node(NodeKind::Gain).expect("add b");
    controller.chain_move_node_by(a, 1).expect("move a after b");

    let ids: Vec<_> = controller.chain().nodes().iter().map(|n| n.id).collect();
    assert_eq!(
        ids,
        vec![b, a],
        "model order must reflect every call, in order"
    );
}

/// C2: `chain_set_param` returns the model's clamped value.
#[test]
fn set_param_returns_clamped() {
    let (mut controller, _dir) = ready_controller();
    let id = controller.chain_add_node(NodeKind::Gain).expect("add");
    let clamped = controller
        .chain_set_param(id, ParamId(0), 999.0)
        .expect("set_param");
    assert_eq!(clamped, 12.0);
}

/// C4 (research R11): a stream rebuild replays the chain onto the fresh
/// `Processor` — a strongly attenuating `Gain` node survives the rebuild
/// and keeps attenuating the signal.
#[test]
fn stream_rebuild_replays_chain() {
    let (mut controller, _dir) = ready_controller();
    let id = controller.chain_add_node(NodeKind::Gain).expect("add");
    controller
        .chain_set_param(id, ParamId(0), -60.0)
        .expect("set_param");

    controller.queue_replace(vec![track("a")]);
    controller.play();
    controller.tick();
    // Drain the insert crossfade + param ramp before measuring.
    let _ = controller.backend_mut().render_buffers(80);
    let attenuated = controller.backend_mut().render_buffers(4);
    let attenuated_peak = attenuated.iter().fold(0.0f32, |m, &s| m.max(s.abs()));

    // Force a stream rebuild (fresh `Processor`, empty `ChainRt` until
    // `replay()` re-populates it) by reconfirming with a different preset.
    let dev_id = DeviceId::new("dev-1").unwrap_or_else(|| unreachable!());
    controller.confirm_device(dev_id, BufferPreset::Safe);
    controller.tick();
    let _ = controller.backend_mut().render_buffers(2); // let the new insert fade settle
    let after_rebuild = controller.backend_mut().render_buffers(4);
    let after_rebuild_peak = after_rebuild.iter().fold(0.0f32, |m, &s| m.max(s.abs()));

    assert!(
        attenuated_peak < 0.1,
        "the -60 dB gain must audibly attenuate before the rebuild: peak={attenuated_peak}"
    );
    assert!(
        after_rebuild_peak < 0.1,
        "replay() must re-insert the same attenuation after the rebuild: peak={after_rebuild_peak}"
    );
}

/// C9: `chain_view()`'s per-node and whole-chain cost read `0.0` while
/// not playing.
#[test]
fn view_reports_zero_cost_when_not_playing() {
    let (mut controller, _dir) = ready_controller();
    controller.chain_add_node(NodeKind::Gain).expect("add");
    // `ready_controller` never calls `play()`, so transport stays Stopped.
    let view = controller.chain_view();
    assert_eq!(view.nodes.len(), 1);
    assert_eq!(view.nodes[0].cost_pct, 0.0);
    assert_eq!(view.total_cost_pct, 0.0);
}

/// C10: the chain is session-scoped — untouched by sign-out — and a new
/// controller starts with an empty one.
#[test]
fn chain_survives_sign_out_and_is_empty_at_construction() {
    let (mut controller, _dir) = ready_controller();
    controller.chain_add_node(NodeKind::Gain).expect("add");
    assert_eq!(controller.chain().nodes().len(), 1);

    controller.clear_for_sign_out();
    assert_eq!(
        controller.chain().nodes().len(),
        1,
        "the chain must survive sign-out (not account-scoped)"
    );

    let (store, _dir2) = fresh_store();
    let fresh = PlaybackController::new(FakeBackend::new(Vec::new()), ScriptedHost::new(), store);
    assert!(
        fresh.chain().nodes().is_empty(),
        "a new controller must start with an empty chain"
    );
}

// -----------------------------------------------------------------------
// research R8: streaming Player under a non-unity tempo — contracts/
// effects-service.md §2 rule C7 (no Player re-seek for tempo, amended
// 2026-09-19) and C8 (engine-gated end-of-track).
// -----------------------------------------------------------------------

/// Inject a fake, atomically-advanceable clock (mirrors
/// `controller_streaming.rs`'s own pattern) and return the offset handle.
fn inject_fake_clock<
    B: modplayer_audio_io::OutputBackend,
    H: modplayer_audio_source::SourceHost,
>(
    controller: &mut PlaybackController<B, H>,
) -> std::sync::Arc<AtomicU64> {
    let base = Instant::now();
    let offset_ms = std::sync::Arc::new(AtomicU64::new(0));
    let clock_offset = std::sync::Arc::clone(&offset_ms);
    controller.set_clock(move || base + Duration::from_millis(clock_offset.load(Ordering::SeqCst)));
    offset_ms
}

/// C7 (as amended after the 2026-09-19 manual walk): a non-unity tempo
/// never makes `tick` re-seek the streaming Player. The receiver is
/// paced by the engine's consumption in both of its feeds, so no
/// drift accrues, and the former predicted-drift re-seek measurably
/// rewound the real-time cursor by the buffered lead on every send
/// (quickstart M2: 0.33 × instead of 0.45 ×). Position follow is the
/// engine anchor's own `advance_rate` (`position_advances_at_ratio_*`).
#[test]
fn player_is_never_reseeked_for_tempo_drift() {
    let (mut controller, handle, _dir) = ready_controller_with_handle();
    controller.queue_replace(vec![track_with_duration("a", 60_000)]);
    controller.play();
    controller.tick();

    let id = controller
        .chain_add_node(NodeKind::TimeStretch)
        .expect("add time stretch");
    controller
        .chain_set_param(id, ParamId(0), 0.5)
        .expect("set ratio to half speed");
    // Render enough buffers to publish a real `advance_rate == 0.5`
    // anchor (a genuine `Processor` runs under `FakeBackend`).
    let _ = controller.backend_mut().render_buffers(40);

    let offset_ms = inject_fake_clock(&mut controller);
    controller.tick();
    let before = handle.record_commands().len();

    // Thirty seconds at half speed would have been fifteen seconds of
    // "predicted drift" — thirty re-seeks under the old rule.
    for elapsed in (1_000..=30_000).step_by(1_000) {
        offset_ms.store(elapsed, Ordering::SeqCst);
        controller.tick();
    }
    let commands = handle.record_commands();
    assert_eq!(
        commands.len(),
        before,
        "a non-unity tempo must not send any Player command, got {:?}",
        &commands[before..]
    );
    assert!(
        !commands.iter().any(|c| matches!(c, SourceCommand::Seek(_))),
        "no SourceCommand::Seek may be sent for tempo follow"
    );
}

/// C7: at unity (`advance_rate == 1.0`, no stretch node engaged) nothing
/// tempo-related is ever sent either — 003/006 behaviour is unchanged.
#[test]
fn no_reseek_at_unity() {
    let (mut controller, handle, _dir) = ready_controller_with_handle();
    controller.queue_replace(vec![track_with_duration("a", 60_000)]);
    controller.play();
    controller.tick();
    let _ = controller.backend_mut().render_buffers(10);

    let offset_ms = inject_fake_clock(&mut controller);
    controller.tick();
    let before = handle.record_commands().len();

    // Ten seconds of elapsed wall time: at unity this must never look
    // like drift, however long it has been since the last (nonexistent)
    // reseek.
    offset_ms.store(10_000, Ordering::SeqCst);
    controller.tick();
    controller.tick();

    assert_eq!(
        handle.record_commands().len(),
        before,
        "unity must never trigger a drift-bounded reseek"
    );
}

/// C8 (`advance_rate < 1.0`): a `SourceEvent::EndOfTrack` that arrives
/// while the engine is still far from the track's end is held rather
/// than mirrored, and mirrored once `tick` observes the engine has since
/// caught up to within the drift bound.
#[test]
fn end_of_track_deferred_at_half_speed() {
    let (mut controller, handle, _dir) = ready_controller_with_handle();
    controller.queue_replace(vec![
        track_with_duration("a", 5_000),
        track_with_duration("b", 5_000),
    ]);
    controller.play();
    controller.tick();
    assert_eq!(
        controller
            .current_track()
            .map(|item| item.track.id.as_str().to_string()),
        Some("spotify:track:a".to_string())
    );

    let id = controller
        .chain_add_node(NodeKind::TimeStretch)
        .expect("add time stretch");
    controller
        .chain_set_param(id, ParamId(0), 0.5)
        .expect("set ratio to half speed");
    // A handful of buffers: engine position is still very early in the
    // 5 s track, nowhere near its end.
    let _ = controller.backend_mut().render_buffers(20);

    handle.emit(SourceEvent::EndOfTrack);
    controller.tick();
    assert_eq!(
        controller
            .current_track()
            .map(|item| item.track.id.as_str().to_string()),
        Some("spotify:track:a".to_string()),
        "an EndOfTrack far from the engine's own end must be deferred, not mirrored"
    );

    // Render deep enough into the track that the engine position is now
    // within the drift bound of its end; the deferred event must then
    // resolve on the very next tick with no further Player event needed.
    let _ = controller.backend_mut().render_buffers(2_000);
    controller.tick();
    assert_eq!(
        controller
            .current_track()
            .map(|item| item.track.id.as_str().to_string()),
        Some("spotify:track:b".to_string()),
        "the deferred EndOfTrack must resolve once the engine catches up"
    );
}

/// C8 (`advance_rate > 1.0`): the engine can reach the track's end before
/// the (real-time) streaming Player does — the controller mirrors
/// `Input::EndOfTrack` itself and then swallows the Player's own,
/// now-late one for the same track rather than double-advancing the
/// queue.
#[test]
fn engine_ends_track_first_at_double_speed_and_player_event_is_swallowed() {
    let (mut controller, handle, _dir) = ready_controller_with_handle();
    controller.queue_replace(vec![
        track_with_duration("a", 300),
        track_with_duration("b", 300),
    ]);
    controller.play();
    controller.tick();

    let id = controller
        .chain_add_node(NodeKind::TimeStretch)
        .expect("add time stretch");
    controller
        .chain_set_param(id, ParamId(0), 2.0)
        .expect("set ratio to double speed");
    // Far more real/output time than the 300 ms track needs even without
    // any tempo change — at 2x the engine position runs well past it.
    let _ = controller.backend_mut().render_buffers(300);

    controller.tick();
    assert_eq!(
        controller
            .current_track()
            .map(|item| item.track.id.as_str().to_string()),
        Some("spotify:track:b".to_string()),
        "the engine must mirror EndOfTrack itself once past the track's end"
    );

    // The (real-time) streaming Player's own EndOfTrack for track "a"
    // arrives late; it must be swallowed, not advance the queue again.
    handle.emit(SourceEvent::EndOfTrack);
    controller.tick();
    assert_eq!(
        controller
            .current_track()
            .map(|item| item.track.id.as_str().to_string()),
        Some("spotify:track:b".to_string()),
        "the Player's late EndOfTrack for the already-ended track must be swallowed"
    );
}

/// research R8: at unity, end-of-track handling is byte-for-byte the
/// existing 003/006 behaviour — a plain `EndOfTrack` immediately advances
/// the queue, with no deferral or swallowing.
#[test]
fn unity_behaviour_unchanged() {
    let (mut controller, handle, _dir) = ready_controller_with_handle();
    controller.queue_replace(vec![
        track_with_duration("a", 60_000),
        track_with_duration("b", 60_000),
    ]);
    controller.play();
    controller.tick();
    let _ = controller.backend_mut().render_buffers(5);

    handle.emit(SourceEvent::EndOfTrack);
    controller.tick();
    assert_eq!(
        controller
            .current_track()
            .map(|item| item.track.id.as_str().to_string()),
        Some("spotify:track:b".to_string()),
        "at unity, EndOfTrack must advance the queue immediately, exactly as before 008"
    );
}

// -----------------------------------------------------------------------
// FR-017/SC-012, contracts/effects-service.md §2 rule C3: `tempo_step`.
// -----------------------------------------------------------------------

/// C3: with a first time-stretch node present, `tempo_step` moves its
/// ratio by exactly `TEMPO_STEP` (0.10) per call, in the requested
/// direction, running FR-008's auto-switch as usual (a plain
/// `chain_set_param`).
#[test]
fn tempo_step_moves_first_time_stretch_by_ten_points() {
    let (mut controller, _dir) = ready_controller();
    let id = controller
        .chain_add_node(NodeKind::TimeStretch)
        .expect("add time stretch");

    controller.tempo_step(1);
    let ratio = controller
        .chain()
        .nodes()
        .iter()
        .find(|n| n.id == id)
        .expect("node")
        .params[0];
    assert!(
        (ratio - 1.10).abs() < 1e-6,
        "one up-step from the 1.0 default must land on 1.10, got {ratio}"
    );

    controller.tempo_step(-1);
    let ratio = controller
        .chain()
        .nodes()
        .iter()
        .find(|n| n.id == id)
        .expect("node")
        .params[0];
    assert!(
        (ratio - 1.00).abs() < 1e-6,
        "a down-step must undo the previous up-step, got {ratio}"
    );
}

/// C3: repeated steps clamp at the catalog's `[0.25, 2.0]` bounds rather
/// than over/undershooting.
#[test]
fn tempo_step_clamps_at_bounds() {
    let (mut controller, _dir) = ready_controller();
    controller
        .chain_add_node(NodeKind::TimeStretch)
        .expect("add time stretch");

    for _ in 0..30 {
        controller.tempo_step(1);
    }
    let ratio = controller.chain().nodes()[0].params[0];
    assert_eq!(ratio, 2.0, "must clamp at the maximum ratio");

    for _ in 0..40 {
        controller.tempo_step(-1);
    }
    let ratio = controller.chain().nodes()[0].params[0];
    assert_eq!(ratio, 0.25, "must clamp at the minimum ratio");
}

/// C3: with no time-stretch node in the chain, `tempo_step` raises the
/// keyed `effects-no-time-stretch` Info exactly once (coalesced: it stays
/// silent while that notification is still visible) and changes nothing.
#[test]
fn tempo_step_without_node_notifies_once() {
    let (mut controller, _dir) = ready_controller();
    controller.chain_add_node(NodeKind::Gain).expect("add gain");

    controller.tempo_step(1);
    let visible_count = controller
        .notifications()
        .visible()
        .filter(|n| n.message_key == KEY_EFFECTS_NO_TIME_STRETCH)
        .count();
    assert_eq!(visible_count, 1, "must raise the notification once");

    controller.tempo_step(1);
    let visible_count = controller
        .notifications()
        .visible()
        .filter(|n| n.message_key == KEY_EFFECTS_NO_TIME_STRETCH)
        .count();
    assert_eq!(
        visible_count, 1,
        "must not raise a second notification while the first is still visible"
    );

    assert_eq!(
        controller.chain().nodes().len(),
        1,
        "tempo_step without a time-stretch node must not add or change any node"
    );
}

// -----------------------------------------------------------------------
// `PlaybackController` — overload/auto-bypass mirroring, contracts/
// effects-service.md §2 rules C5/C6 (research R10).
// -----------------------------------------------------------------------

/// C5: an `Event::Overload` raises the keyed `effect-chain-over-budget`
/// warning exactly once while it stays visible, even across repeated
/// events (the RT keeps reporting the overload every render it persists).
#[test]
fn overload_event_raises_one_keyed_warning() {
    let (mut controller, _dir) = ready_controller();
    let id = controller.chain_add_node(NodeKind::Gain).expect("add");
    let slot = controller
        .chain()
        .nodes()
        .iter()
        .find(|n| n.id == id)
        .expect("node")
        .slot;

    // Mirrors production ordering (`Processor::render` publishes
    // `over_budget` before pushing the event the same render).
    controller.shared().set_over_budget(true);
    controller.debug_inject_engine_event(Event::Overload {
        costliest_slot: slot,
        render_pct: 123,
    });
    controller.tick();

    let warnings: Vec<_> = controller
        .notifications()
        .visible()
        .filter(|n| n.message_key == KEY_EFFECT_CHAIN_OVER_BUDGET)
        .collect();
    assert_eq!(warnings.len(), 1, "exactly one keyed warning");
    assert_eq!(warnings[0].severity, Severity::Warning);

    controller.debug_inject_engine_event(Event::Overload {
        costliest_slot: slot,
        render_pct: 150,
    });
    controller.tick();
    let count = controller
        .notifications()
        .visible()
        .filter(|n| n.message_key == KEY_EFFECT_CHAIN_OVER_BUDGET)
        .count();
    assert_eq!(count, 1, "must not duplicate while still visible");
}

/// C5: an `Event::AutoBypassed` marks the model's node bypassed/
/// auto-bypassed (so the panel's label follows) and raises the keyed
/// `effect-chain-auto-bypassed` warning.
#[test]
fn auto_bypassed_event_marks_model() {
    let (mut controller, _dir) = ready_controller();
    let id = controller.chain_add_node(NodeKind::Gain).expect("add");
    let slot = controller
        .chain()
        .nodes()
        .iter()
        .find(|n| n.id == id)
        .expect("node")
        .slot;

    controller.debug_inject_engine_event(Event::AutoBypassed { slot });
    controller.tick();

    let node = controller
        .chain()
        .nodes()
        .iter()
        .find(|n| n.id == id)
        .expect("node still present");
    assert!(node.bypassed, "must be bypassed");
    assert!(node.auto_bypassed, "must be flagged auto-bypassed");

    let raised = controller
        .notifications()
        .visible()
        .filter(|n| n.message_key == KEY_EFFECT_CHAIN_AUTO_BYPASSED)
        .count();
    assert_eq!(raised, 1, "must raise the keyed warning");
}

/// C6: `tick` dismisses the over-budget warning once `RtShared::
/// over_budget()` reports a clean window.
#[test]
fn warning_clears_when_rt_reports_clean_window() {
    let (mut controller, _dir) = ready_controller();
    let id = controller.chain_add_node(NodeKind::Gain).expect("add");
    let slot = controller
        .chain()
        .nodes()
        .iter()
        .find(|n| n.id == id)
        .expect("node")
        .slot;

    controller.shared().set_over_budget(true);
    controller.debug_inject_engine_event(Event::Overload {
        costliest_slot: slot,
        render_pct: 123,
    });
    controller.tick();
    assert_eq!(
        controller
            .notifications()
            .visible()
            .filter(|n| n.message_key == KEY_EFFECT_CHAIN_OVER_BUDGET)
            .count(),
        1,
        "must be visible right after the overload"
    );

    controller.shared().set_over_budget(false);
    controller.tick();
    assert_eq!(
        controller
            .notifications()
            .visible()
            .filter(|n| n.message_key == KEY_EFFECT_CHAIN_OVER_BUDGET)
            .count(),
        0,
        "must dismiss once the RT reports a clean window"
    );
}

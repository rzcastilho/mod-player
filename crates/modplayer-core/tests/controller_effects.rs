// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! contracts/effects-service.md §1 (`ChainModel`, rules G1-G9) and §2
//! (`PlaybackController` façade, rules C1, C2, C4, C9, C10).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use modplayer_audio_io::{FakeBackend, FakeDevice};
use modplayer_audio_source::{Availability, SourceCommand, SourceEvent, TrackId, TrackRef};
use modplayer_audio_source_synthetic::{ScriptedHost, ScriptedHostHandle};
use modplayer_core::notifications::{
    KEY_EFFECT_CHAIN_AUTO_BYPASSED, KEY_EFFECT_CHAIN_OVER_BUDGET, KEY_EFFECTS_NO_TIME_STRETCH,
};
use modplayer_core::plugins::{Lifecycle, PluginId};
use modplayer_core::settings::SettingsStore;
use modplayer_core::{ChainError, ChainModel, PlaybackController, Severity};
use modplayer_effects::catalog::{self, NodeKind, NodeOwner, ParamId, QualityMode};
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
/// Clears every discovered record's `enabled` flag so `launch()` spawns
/// no plugin thread: this file asserts on exactly the nodes it adds to
/// the chain itself, and Key & Tempo (013) would otherwise add its own
/// pitch/stretch pair asynchronously from `ready_ack` (the fixture-driven
/// tests further down use their own harness and opt fixtures back in).
fn leave_plugins_unlaunched(controller: &mut PlaybackController<FakeBackend, ScriptedHost>) {
    let ids: Vec<_> = controller
        .plugins_mut()
        .records()
        .iter()
        .map(|r| r.id)
        .collect();
    for id in ids {
        if let Some(record) = controller.plugins_mut().record_mut(id) {
            record.enabled = false;
        }
    }
}

fn ready_controller() -> (PlaybackController<FakeBackend, ScriptedHost>, TempDir) {
    let (store, dir) = fresh_store();
    let host = ScriptedHost::new();
    let devices = vec![fake_device()];
    let mut controller = PlaybackController::new(FakeBackend::new(devices), host, store);
    leave_plugins_unlaunched(&mut controller);
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
    leave_plugins_unlaunched(&mut controller);
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

// -----------------------------------------------------------------------
// 013-key-and-tempo-plugin (API 1.4, research R1/R3, contract
// plugin-api-v1.4.md §8): `effect_chain_changed` fans out on a parameter/
// mode change by any actor, coalesced to one per tick, carrying the
// clamped target immediately — proven against a real subscribed plugin
// thread, the `effects-observer` fixture (`MODPLAYER_PLUGIN_FIXTURES=1`),
// so the R4 delivery fix and the R3 revision-bump rule are both exercised
// end to end ahead of the bundled Key & Tempo package existing.
// -----------------------------------------------------------------------

const EFFECTS_OBSERVER: &str = "org.modplayer.fixture.effects-observer";

static FIXTURES_ENV_LOCK: Mutex<()> = Mutex::new(());

/// As [`ready_controller`], but with fixtures enabled — the
/// `effects-observer` fixture is what receives every `effect_chain_changed`
/// these tests assert on.
fn fixtures_ready_controller() -> (
    PlaybackController<FakeBackend, ScriptedHost>,
    TempDir,
    TempDir,
    TempDir,
) {
    let (store, dir) = fresh_store();
    let plugin_state_dir = TempDir::new();
    let track_state_dir = TempDir::new();
    let host = ScriptedHost::new();
    let devices = vec![fake_device()];
    let mut controller = {
        let _guard = FIXTURES_ENV_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        // Safety: narrowly scopes each mutation to the one synchronous
        // read `PlaybackController::new` makes of it, serialized against
        // every other test in this binary via the lock above (mirrors
        // `controller_plugins_permissions.rs`'s own `fixture_controller`).
        unsafe {
            std::env::set_var("MODPLAYER_PLUGIN_FIXTURES", "1");
            std::env::set_var("MODPLAYER_PLUGIN_STATE_DIR", plugin_state_dir.path());
            std::env::set_var("MODPLAYER_TRACK_STATE_DIR", track_state_dir.path());
        }
        let controller = PlaybackController::new(FakeBackend::new(devices), host, store);
        unsafe {
            std::env::remove_var("MODPLAYER_PLUGIN_FIXTURES");
            std::env::remove_var("MODPLAYER_PLUGIN_STATE_DIR");
            std::env::remove_var("MODPLAYER_TRACK_STATE_DIR");
        }
        controller
    };
    // The tests below exist specifically to prove the R3/R4 mechanism
    // against the `effects-observer` fixture *in isolation* (this file's
    // own header comment: "ahead of the bundled Key & Tempo package
    // existing"). With every fixture *and* both bundled packages
    // discovered (`MODPLAYER_PLUGIN_FIXTURES=1`), several other
    // `audio.effects`-holding packages would otherwise also touch the
    // chain on their own `ready_ack` — the `wellbehaved` fixture creates
    // and parameterises a pitch_shift/time_stretch pair of its own (US3
    // T091), and 013-key-and-tempo-plugin's own bundled Key & Tempo (G2)
    // does the same — racing the exact-event-count assertions these
    // tests make. Disabled here, before `launch()` ever spawns anything,
    // so nothing but the observer itself ever runs — zero-race, not a
    // timing workaround.
    let other_ids: Vec<_> = controller
        .plugins_mut()
        .records()
        .iter()
        .filter(|r| r.identifier.as_str() != EFFECTS_OBSERVER)
        .map(|r| r.id)
        .collect();
    for id in other_ids {
        if let Some(record) = controller.plugins_mut().record_mut(id) {
            record.enabled = false;
        }
    }
    controller.launch();
    let dev_id = DeviceId::new("dev-1").unwrap_or_else(|| unreachable!());
    controller.confirm_device(dev_id, BufferPreset::Balanced);
    (controller, dir, plugin_state_dir, track_state_dir)
}

fn effects_observer_id(controller: &mut PlaybackController<FakeBackend, ScriptedHost>) -> PluginId {
    controller
        .plugins_mut()
        .records()
        .iter()
        .find(|r| r.identifier.as_str() == EFFECTS_OBSERVER)
        .map(|r| r.id)
        .unwrap_or_else(|| unreachable!("{EFFECTS_OBSERVER} must be discovered"))
}

fn pump_until(
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

/// Waits for the observer to be `Active`, so every test starts from a
/// clean, ready-to-receive state.
fn wait_observer_active(controller: &mut PlaybackController<FakeBackend, ScriptedHost>) {
    let id = effects_observer_id(controller);
    assert!(
        pump_until(controller, Duration::from_secs(2), |c| matches!(
            c.plugins_mut().record(id).map(|r| &r.lifecycle),
            Some(Lifecycle::Active)
        )),
        "the effects-observer fixture must reach Active"
    );
}

/// Drains a handful of idle ticks so any `effect_chain_changed` already
/// in flight from a just-issued structural edit (e.g. `chain_add_node`)
/// lands and is logged before a test captures its own "before" baseline
/// — otherwise that delivery could arrive during the test's own window
/// and be mistaken for the one it triggers.
fn settle(controller: &mut PlaybackController<FakeBackend, ScriptedHost>) {
    for _ in 0..5 {
        controller.tick();
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Every `effect_chain_changed#<seq>:...` line the observer has logged so
/// far, in order.
fn observer_log_lines(controller: &PlaybackController<FakeBackend, ScriptedHost>) -> Vec<String> {
    controller
        .plugin_log()
        .entries()
        .filter(|e| e.message.starts_with("effect_chain_changed#"))
        .map(|e| e.message.clone())
        .collect()
}

/// Extracts a numeric `field=<value>` from one of the observer's log
/// lines — tolerant of however Luau chooses to render a float (`1.1` vs
/// `1.1000000`), unlike a literal string match.
fn field_value(log: &str, field: &str) -> f64 {
    let marker = format!("{field}=");
    let start = log
        .find(&marker)
        .unwrap_or_else(|| unreachable!("field '{field}' not found in {log:?}"))
        + marker.len();
    let rest = &log[start..];
    let end = rest.find([',', ']']).unwrap_or(rest.len());
    rest[..end]
        .parse::<f64>()
        .unwrap_or_else(|_| unreachable!("field '{field}' not numeric in {log:?}"))
}

/// research R3/R7: the host's `tempo_step` action (008 FR-017) bumps
/// `revision` on the changed `ratio` target, which fans out exactly one
/// `effect_chain_changed` carrying it — the same mechanism a plugin's own
/// `set_param`, an Effect Chain panel edit, or auto-switch/rate-change all
/// share (one bump site, `ChainModel::set_param`/`set_mode`/
/// `set_source_rate`).
#[test]
fn tempo_step_fans_out_effect_chain_changed_with_ratio() {
    let (mut controller, _dir, _psd, _tsd) = fixtures_ready_controller();
    wait_observer_active(&mut controller);

    controller
        .chain_add_node(NodeKind::TimeStretch)
        .unwrap_or_else(|e| unreachable!("{e:?}"));
    settle(&mut controller);
    let before = observer_log_lines(&controller).len();

    controller.tempo_step(1);
    assert!(
        pump_until(&mut controller, Duration::from_secs(2), |c| {
            observer_log_lines(c).len() > before
        }),
        "tempo_step must fan out effect_chain_changed"
    );

    let lines = observer_log_lines(&controller);
    let last = lines.last().unwrap_or_else(|| unreachable!());
    assert!(
        last.contains("time_stretch["),
        "expected a time_stretch node in the payload: {last}"
    );
    let ratio = field_value(last, "ratio");
    assert!(
        (ratio - 1.10).abs() < 1e-6,
        "expected ratio ~= 1.10 (1.0 + TEMPO_STEP), got {ratio} ({last})"
    );
}

/// research R3, contract §4.1: an Effect Chain panel edit — `chain_set_
/// param`/`chain_set_mode` from the UI path, exactly as `effects_view.rs`
/// calls them — fans out `effect_chain_changed` the same as any other
/// actor; an explicit mode choice clears `auto_switched`.
#[test]
fn panel_edit_fans_out_effect_chain_changed() {
    let (mut controller, _dir, _psd, _tsd) = fixtures_ready_controller();
    wait_observer_active(&mut controller);

    let id = controller
        .chain_add_node(NodeKind::PitchShift)
        .unwrap_or_else(|e| unreachable!("{e:?}"));
    settle(&mut controller);
    let before = observer_log_lines(&controller).len();

    // A stage-use excursion (panel slider edit) auto-switches to Quality.
    controller
        .chain_set_param(id, ParamId(0), 7.0)
        .unwrap_or_else(|e| unreachable!("{e:?}"));
    assert!(
        pump_until(&mut controller, Duration::from_secs(2), |c| {
            observer_log_lines(c).len() > before
        }),
        "the panel's set_param must fan out effect_chain_changed"
    );
    let after_excursion = observer_log_lines(&controller);
    let last = after_excursion.last().unwrap_or_else(|| unreachable!());
    assert!(
        last.contains("auto_switched=true"),
        "the auto-switch must be visible in the payload: {last}"
    );

    // An explicit mode choice (the panel's Quality dropdown) clears
    // `auto_switched`.
    let before2 = after_excursion.len();
    controller
        .chain_set_mode(id, QualityMode::Performance)
        .unwrap_or_else(|e| unreachable!("{e:?}"));
    assert!(
        pump_until(&mut controller, Duration::from_secs(2), |c| {
            observer_log_lines(c).len() > before2
        }),
        "the panel's set_mode must fan out effect_chain_changed"
    );
    let lines = observer_log_lines(&controller);
    let last = lines.last().unwrap_or_else(|| unreachable!());
    assert!(
        last.contains("auto_switched=false"),
        "an explicit mode choice must clear auto_switched: {last}"
    );
}

/// research R3: three parameter writes landing in the same tick (before
/// `tick()` ever runs) still bump `revision` no more than the once the
/// controller's own diff observes — coalesced to exactly one
/// `effect_chain_changed`, not three.
#[test]
fn param_changes_coalesce_to_one_event_per_tick() {
    let (mut controller, _dir, _psd, _tsd) = fixtures_ready_controller();
    wait_observer_active(&mut controller);

    let id = controller
        .chain_add_node(NodeKind::Gain)
        .unwrap_or_else(|e| unreachable!("{e:?}"));
    settle(&mut controller);
    let before = observer_log_lines(&controller).len();

    // Three writes, no `tick()` between them.
    controller
        .chain_set_param(id, ParamId(0), -10.0)
        .unwrap_or_else(|e| unreachable!("{e:?}"));
    controller
        .chain_set_param(id, ParamId(0), -20.0)
        .unwrap_or_else(|e| unreachable!("{e:?}"));
    controller
        .chain_set_param(id, ParamId(0), -30.0)
        .unwrap_or_else(|e| unreachable!("{e:?}"));

    assert!(
        pump_until(&mut controller, Duration::from_secs(2), |c| {
            observer_log_lines(c).len() > before
        }),
        "the coalesced write must eventually fan out"
    );
    // Give the plugin thread a couple more idle ticks to prove no further
    // delivery follows (it would if each write had fanned out its own
    // event instead of being coalesced by the revision diff).
    for _ in 0..5 {
        controller.tick();
        std::thread::sleep(Duration::from_millis(2));
    }
    let lines = observer_log_lines(&controller);
    assert_eq!(
        lines.len(),
        before + 1,
        "three writes before one tick must coalesce to exactly one event: {lines:?}"
    );
    let last = lines.last().unwrap_or_else(|| unreachable!());
    let level = field_value(last, "level");
    assert!(
        (level - -30.0).abs() < 1e-6,
        "the coalesced event must carry the final, clamped value: {last}"
    );
}

/// research R3/Constitution I: the value `effect_chain_changed` carries
/// is `ChainModel`'s control-side clamped *target*, available the very
/// tick the write lands — never a mid-ramp RT value, so no wait for the
/// 20 ms ramp is needed for it to be correct.
#[test]
fn effect_chain_changed_params_are_targets_not_ramp() {
    let (mut controller, _dir, _psd, _tsd) = fixtures_ready_controller();
    wait_observer_active(&mut controller);

    let id = controller
        .chain_add_node(NodeKind::Gain)
        .unwrap_or_else(|e| unreachable!("{e:?}"));
    settle(&mut controller);
    let before = observer_log_lines(&controller).len();

    let clamped = controller
        .chain_set_param(id, ParamId(0), -6.0)
        .unwrap_or_else(|e| unreachable!("{e:?}"));

    // Exactly one more tick — deliberately not waiting out the 20 ms ramp
    // (008 FR-010) — the fan-out already happened on the tick the write
    // landed in.
    controller.tick();
    assert!(
        pump_until(&mut controller, Duration::from_millis(200), |c| {
            observer_log_lines(c).len() > before
        }),
        "the write must fan out well within the 20 ms ramp window"
    );
    let lines = observer_log_lines(&controller);
    let last = lines.last().unwrap_or_else(|| unreachable!());
    let level = field_value(last, "level");
    assert!(
        (f64::from(clamped) - level).abs() < 1e-6,
        "the event must carry the clamped target immediately, not a ramping value: {last}"
    );
}

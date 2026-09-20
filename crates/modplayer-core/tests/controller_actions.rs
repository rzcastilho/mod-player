// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

//! `PlaybackController` Action & Binding façade tests (007,
//! contracts/action-registry.md §5-§6).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use modplayer_audio_io::{FakeBackend, FakeDevice};
use modplayer_audio_source::{Availability, TrackId, TrackRef};
use modplayer_audio_source_synthetic::{ScriptedHost, ScriptedHostHandle};
use modplayer_core::settings::SettingsStore;
use modplayer_core::{Chord, HostAction, KeymapOverrides, PlaybackController, def};
use modplayer_engine::{BufferPreset, DeviceId, FrameCount, SampleRate, VolumePercent};

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-controller-actions-{}-{}",
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
        name: "dev-1".to_string(),
        rate: SampleRate::new(44_100),
        channels: 2,
        buffer_range: Some((FrameCount::new(32), FrameCount::new(2_048))),
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

fn chord(s: &str) -> Chord {
    Chord::parse(s).unwrap_or_else(|_| unreachable!("{s:?} must be valid grammar"))
}

/// A controller over a confirmed device with transport enabled and a
/// current track playing (mirrors `controller_streaming.rs`'s
/// `unavailable_disables_transport_and_keeps_navigation` setup).
fn ready_controller() -> (
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
    controller.set_playback_permitted(true, None);
    controller.tick();
    controller.queue_replace(vec![track("a")]);
    controller.play();
    controller.tick();
    assert!(
        controller.transport_enabled(),
        "test setup must reach transport_enabled"
    );
    (controller, handle, dir)
}

// ---------------------------------------------------------------------
// T031
// ---------------------------------------------------------------------

#[test]
fn add_binding_persists_and_applies_in_same_call() {
    let (mut controller, _handle, dir) = ready_controller();
    let k = chord("K");

    let result = controller.add_binding(HostAction::ToggleLoop, k);

    assert_eq!(result, Ok(()));
    // Applies immediately: the registry reflects it in this same call,
    // no reload needed (FR-005 sync dispatch).
    assert_eq!(
        controller.actions().bindings(HostAction::ToggleLoop),
        &[chord("L"), k]
    );

    // Persists: reconstructing a controller from the same settings path
    // sees the same customisation.
    let store = SettingsStore::with_path(dir.path().join("settings.toml"));
    let outcome = store.load();
    assert!(outcome.warnings.is_empty());
    assert_eq!(
        outcome
            .settings
            .keybinding_overrides
            .get(HostAction::ToggleLoop),
        Some([chord("L"), k].as_slice())
    );
}

#[test]
fn remove_reset_and_reset_all_bindings_persist() {
    let (mut controller, _handle, dir) = ready_controller();
    let store_path = dir.path().join("settings.toml");

    controller
        .add_binding(HostAction::ToggleLoop, chord("K"))
        .unwrap_or_else(|_| unreachable!());
    controller.remove_binding(HostAction::ToggleLoop, chord("L"));
    assert_eq!(
        controller.actions().bindings(HostAction::ToggleLoop),
        &[chord("K")]
    );
    let outcome = SettingsStore::with_path(&store_path).load();
    assert_eq!(
        outcome
            .settings
            .keybinding_overrides
            .get(HostAction::ToggleLoop),
        Some([chord("K")].as_slice())
    );

    controller.reset_binding(HostAction::ToggleLoop);
    assert_eq!(
        controller.actions().bindings(HostAction::ToggleLoop),
        &[chord("L")]
    );
    let outcome = SettingsStore::with_path(&store_path).load();
    assert!(outcome.settings.keybinding_overrides.is_empty());

    controller
        .add_binding(HostAction::AddPointMarker, chord("N"))
        .unwrap_or_else(|_| unreachable!());
    controller.reset_all_bindings();
    let outcome = SettingsStore::with_path(&store_path).load();
    assert!(outcome.settings.keybinding_overrides.is_empty());
}

#[test]
fn set_action_enabled_is_not_persisted() {
    let (mut controller, _handle, dir) = ready_controller();
    // 008 flips TempoStepUp's own shipped default to enabled (T049); this
    // test now disables it (rather than enabling it) to exercise the same
    // "the mutation isn't persisted" behaviour.
    assert!(controller.actions().is_enabled(HostAction::TempoStepUp));

    controller.set_action_enabled(HostAction::TempoStepUp, false);
    assert!(!controller.actions().is_enabled(HostAction::TempoStepUp));

    // Reconstructing from the same settings must not remember it: a
    // fresh registry re-seeds `enabled` from the catalog default.
    let store = SettingsStore::with_path(dir.path().join("settings.toml"));
    let host = ScriptedHost::new();
    let reconstructed = PlaybackController::new(FakeBackend::new(vec![fake_device()]), host, store);
    assert!(reconstructed.actions().is_enabled(HostAction::TempoStepUp));
}

/// `position()` after a seek needs one `tick()` (to drain the reducer)
/// plus a render pass (the audio clock advances from rendered frames) —
/// same pattern as `controller_streaming.rs::seek_on_buffered_audio_
/// lands_at_the_requested_position`. Asserts within 50 ms, matching that
/// test's own tolerance.
fn assert_position_near(
    controller: &mut PlaybackController<FakeBackend, ScriptedHost>,
    expected: Duration,
) {
    controller.tick();
    let _ = controller.backend_mut().render_buffers(1);
    let position_ms = controller.position().as_millis() as i128;
    let expected_ms = expected.as_millis() as i128;
    assert!(
        (expected_ms - 50..=expected_ms + 50).contains(&position_ms),
        "expected position near {expected_ms}ms, got {position_ms}ms"
    );
}

#[test]
fn seek_step_moves_five_seconds_and_clamps() {
    let (mut controller, _handle, _dir) = ready_controller();

    controller.seek_step(1);
    assert_position_near(&mut controller, Duration::from_secs(5));

    controller.seek_step(-1);
    assert_position_near(&mut controller, Duration::ZERO);

    // Backward past zero clamps at zero, never underflows/panics.
    controller.seek_step(-1);
    assert_position_near(&mut controller, Duration::ZERO);

    // Forward past the track end clamps at the track's length (180 s).
    // `position()` only reflects a seek once the engine has processed it
    // (`tick()` + a render pass), so each step must land before the next
    // one reads `position()` off it.
    for _ in 0..40 {
        controller.seek_step(1);
        controller.tick();
        let _ = controller.backend_mut().render_buffers(1);
    }
    assert_position_near(&mut controller, Duration::from_secs(180));
}

#[test]
fn volume_step_moves_five_percent_and_saturates() {
    let (mut controller, _handle, _dir) = ready_controller();
    controller.set_master_volume(VolumePercent::new(50));

    controller.step_master_volume(1);
    assert_eq!(controller.master_volume().value(), 55);

    controller.step_master_volume(-1);
    assert_eq!(controller.master_volume().value(), 50);

    for _ in 0..20 {
        controller.step_master_volume(1);
    }
    assert_eq!(
        controller.master_volume().value(),
        100,
        "must saturate at 100"
    );

    for _ in 0..30 {
        controller.step_master_volume(-1);
    }
    assert_eq!(controller.master_volume().value(), 0, "must saturate at 0");
}

// ---------------------------------------------------------------------
// T076 (US4 AS1, SC-004): restart persistence through the real
// controller/settings-store round trip, not just `KeymapOverrides` or
// `SettingsStore` in isolation (T024/T027 cover those).
// ---------------------------------------------------------------------

#[test]
fn custom_binding_survives_controller_restart() {
    let (mut controller, _handle, dir) = ready_controller();
    let store_path = dir.path().join("settings.toml");
    let k = chord("K");

    controller
        .add_binding(HostAction::ToggleLoop, k)
        .unwrap_or_else(|_| unreachable!());
    drop(controller);

    // Reconstruct a whole new controller from the same settings-store
    // path, exactly like relaunching the app.
    let store = SettingsStore::with_path(&store_path);
    let host = ScriptedHost::new();
    let reconstructed = PlaybackController::new(FakeBackend::new(vec![fake_device()]), host, store);

    let mut expected_overrides = KeymapOverrides::default();
    expected_overrides.set(HostAction::ToggleLoop, vec![chord("L"), k]);
    assert_eq!(
        reconstructed.actions().overrides(),
        &expected_overrides,
        "exactly one action must be customised after restart"
    );

    // Every other action's effective bindings are still the untouched
    // catalog default.
    for action in HostAction::ALL {
        if action == HostAction::ToggleLoop {
            continue;
        }
        let expected: Vec<Chord> = def(action)
            .default_bindings
            .iter()
            .map(|s| chord(s))
            .collect();
        assert_eq!(
            reconstructed.actions().bindings(action),
            expected.as_slice(),
            "{action:?} must remain at its catalog default after restart"
        );
    }
}

#[test]
fn seek_and_volume_steps_are_no_ops_while_transport_disabled() {
    let (store, _dir) = fresh_store();
    let host = ScriptedHost::new();
    // No device confirmed, no track: transport must be disabled.
    let mut controller = PlaybackController::new(FakeBackend::new(vec![]), host, store);
    assert!(!controller.transport_enabled());

    controller.set_master_volume(VolumePercent::new(50));
    controller.step_master_volume(1);
    assert_eq!(
        controller.master_volume().value(),
        50,
        "volume step must be a no-op while transport is disabled"
    );

    controller.seek_step(1);
    assert_eq!(controller.position(), Duration::ZERO);
}

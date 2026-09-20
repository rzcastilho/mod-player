// SPDX-License-Identifier: MIT OR Apache-2.0

//! Device Check integration tests (spec US1 acceptance 1, 3, 4, 5; edge
//! case "zero devices"): fresh install shows Device Check with the system
//! default previewed and the test tone auto-played; "Yes" persists the
//! device/preset/confirmation and skips Device Check on relaunch; "Skip for
//! now" leaves the device unconfirmed so the screen reappears; zero devices
//! at launch produce the empty state with no tone attempted and a Critical
//! notification.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use modplayer_audio_io::FakeBackend;
use modplayer_audio_source::{Availability, TrackId, TrackRef};
use modplayer_audio_source_synthetic::SyntheticHost;
use modplayer_core::settings::SettingsStore;
use modplayer_core::{PlaybackController, Severity};
use modplayer_engine::{BufferPreset, DeviceId, SampleRate, Transport};

/// A single fixture track (003 T047 changed `play()` to require a current
/// queue item, contracts/transport-and-queue.md §2 rule T1) — this file's
/// tests only care about device/stream rebuild behaviour, not queue
/// content, so one arbitrary track is enough to make `play()` proceed.
fn fixture_track() -> TrackRef {
    TrackRef::new(
        TrackId::new("spotify:track:fixture").unwrap_or_else(|_| unreachable!()),
        "Fixture",
        vec!["Fixture Artist".to_string()],
        None,
        None,
        180_000,
        Availability::Available,
    )
}

/// A minimal self-cleaning temp directory (no `tempfile` dependency),
/// mirroring `tests/settings.rs`'s isolation approach.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-device-policy-integration-{}-{}",
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

fn fake_device(id: &str, name: &str, is_default: bool) -> modplayer_audio_io::FakeDevice {
    use modplayer_engine::{FrameCount, SampleRate};
    modplayer_audio_io::FakeDevice {
        id: DeviceId::new(id).unwrap_or_else(|| unreachable!("test id is non-empty")),
        name: name.to_string(),
        rate: SampleRate::new(44_100),
        channels: 2,
        buffer_range: Some((FrameCount::new(32), FrameCount::new(2048))),
        is_default,
    }
}

#[test]
fn fresh_install_previews_default_and_auto_plays_test_tone() {
    let (store, _dir) = fresh_store();
    let devices = vec![fake_device("dev-1", "Speakers", true)];
    let mut controller =
        PlaybackController::new(FakeBackend::new(devices), SyntheticHost::new(44_100), store);

    controller.launch();

    assert!(
        controller.should_show_device_check(),
        "fresh install must show Device Check"
    );
    assert!(
        controller.is_connected(),
        "default device must be previewed"
    );
    assert_eq!(
        controller.active_device().map(|d| d.is_fallback),
        Some(false),
        "the not-yet-confirmed preview is not a fallback"
    );

    // The tone auto-plays as part of `launch()`; driving buffers must make
    // it audible on the shared peak meter.
    let mut peak_seen = false;
    for _ in 0..50 {
        controller.backend_mut().render_buffers(1);
        if controller.shared().peak() > 0.0 {
            peak_seen = true;
            break;
        }
    }
    assert!(peak_seen, "auto-played test tone should be audible");
}

#[test]
fn yes_persists_device_and_skips_device_check_next_launch() {
    let (store, _dir) = fresh_store();
    let devices = vec![fake_device("dev-1", "Speakers", true)];

    let mut controller = PlaybackController::new(
        FakeBackend::new(devices.clone()),
        SyntheticHost::new(44_100),
        store.clone(),
    );
    controller.launch();
    let confirmed_id = DeviceId::new("dev-1").unwrap_or_else(|| unreachable!());
    controller.confirm_device(confirmed_id.clone(), BufferPreset::Performance);

    assert!(!controller.should_show_device_check());
    assert_eq!(controller.preferred_device(), Some(&confirmed_id));
    assert!(controller.device_confirmed());

    // Simulate relaunch: a brand-new controller loading the same settings file.
    let mut relaunched =
        PlaybackController::new(FakeBackend::new(devices), SyntheticHost::new(44_100), store);
    relaunched.launch();

    assert!(
        !relaunched.should_show_device_check(),
        "confirmed device must skip Device Check on relaunch"
    );
    assert_eq!(relaunched.preferred_device(), Some(&confirmed_id));
    assert_eq!(relaunched.preset(), BufferPreset::Performance);
    assert_eq!(
        relaunched.active_device().map(|d| d.is_fallback),
        Some(false)
    );
}

#[test]
fn skip_for_now_leaves_unconfirmed_and_device_check_reappears() {
    let (store, _dir) = fresh_store();
    let devices = vec![fake_device("dev-1", "Speakers", true)];

    let mut controller = PlaybackController::new(
        FakeBackend::new(devices.clone()),
        SyntheticHost::new(44_100),
        store.clone(),
    );
    controller.launch();
    controller.skip_device_check();

    assert!(!controller.device_confirmed());

    let mut relaunched =
        PlaybackController::new(FakeBackend::new(devices), SyntheticHost::new(44_100), store);
    relaunched.launch();

    assert!(
        relaunched.should_show_device_check(),
        "skipping must leave the screen reappearing next launch"
    );
    assert_eq!(relaunched.preferred_device(), None);
}

#[test]
fn zero_devices_at_launch_is_empty_state_with_no_tone_and_critical_notice() {
    let (store, _dir) = fresh_store();
    let mut controller =
        PlaybackController::new(FakeBackend::new(vec![]), SyntheticHost::new(44_100), store);

    controller.launch();

    assert!(controller.should_show_device_check());
    assert!(!controller.is_connected());
    assert!(!controller.transport_available());
    assert!(
        !controller.play_test_tone(),
        "no device exists to attempt a tone on"
    );

    let notification = controller.notifications().visible().next();
    assert_eq!(notification.map(|n| n.severity), Some(Severity::Critical));
    assert_eq!(
        notification.map(|n| n.message_key),
        Some("no-output-devices")
    );
}

// --- User Story 3: output device resilience (spec US3 acceptance 1-5; ---
// --- FR-014, FR-013; contracts/output-backend.md) ------------------------

/// Contract-named test (contracts/output-backend.md's test list): the
/// zero-devices-at-launch behaviour above, pinned again under the exact
/// name US3's contract expects for traceability.
#[test]
fn zero_devices_at_launch() {
    let (store, _dir) = fresh_store();
    let mut controller =
        PlaybackController::new(FakeBackend::new(vec![]), SyntheticHost::new(44_100), store);

    controller.launch();

    assert!(!controller.is_connected());
    assert!(!controller.transport_available());
    let notification = controller.notifications().visible().next();
    assert_eq!(notification.map(|n| n.severity), Some(Severity::Critical));
    assert_eq!(
        notification.map(|n| n.message_key),
        Some("no-output-devices")
    );
}

/// US3 acceptance 5 / FR-014: the remembered device is absent at launch —
/// the system default is used, a Warning names the situation, the
/// preference is retained, and Device Check is not re-shown. (This
/// resolution path predates this phase; the test is added here for US3's
/// contract traceability. The exact device named in the warning is
/// Foundational/US1 behaviour and is intentionally not pinned here.)
#[test]
fn missing_preferred_at_launch_uses_default_with_warning() {
    let (store, _dir) = fresh_store();
    let missing_id = DeviceId::new("missing-dev").unwrap_or_else(|| unreachable!());
    {
        let mut controller = PlaybackController::new(
            FakeBackend::new(vec![fake_device("missing-dev", "Old Interface", true)]),
            SyntheticHost::new(44_100),
            store.clone(),
        );
        controller.launch();
        controller.confirm_device(missing_id.clone(), BufferPreset::Balanced);
    }

    // Relaunch with a different device list — the preferred device is gone.
    let devices = vec![fake_device("dev-2", "Built-in Speakers", true)];
    let mut controller =
        PlaybackController::new(FakeBackend::new(devices), SyntheticHost::new(44_100), store);
    controller.launch();

    assert!(
        !controller.should_show_device_check(),
        "a missing preferred device must not re-show Device Check"
    );
    assert_eq!(
        controller.preferred_device(),
        Some(&missing_id),
        "the stored preference is retained, not overwritten by the fallback"
    );
    assert_eq!(
        controller.active_device().map(|d| d.is_fallback),
        Some(true)
    );

    let notification = controller.notifications().visible().next();
    assert_eq!(notification.map(|n| n.severity), Some(Severity::Warning));
    assert_eq!(
        notification.map(|n| n.message_key),
        Some("device-missing-at-launch")
    );
}

/// US3 acceptance 1: the active device disappears mid-session; output
/// falls back to the system default within the same tick, a Critical
/// notification names the lost device, and the audio clock is not reset.
#[test]
fn fallback_on_device_lost() {
    let (store, _dir) = fresh_store();
    let lost_id = DeviceId::new("dev-1").unwrap_or_else(|| unreachable!());
    let devices = vec![
        fake_device("dev-1", "USB Interface", true),
        fake_device("dev-2", "Built-in Speakers", false),
    ];
    let mut controller =
        PlaybackController::new(FakeBackend::new(devices), SyntheticHost::new(44_100), store);
    controller.launch();
    controller.confirm_device(lost_id.clone(), BufferPreset::Balanced);
    controller.queue_replace(vec![fixture_track()]);
    controller.play();

    controller.backend_mut().render_buffers(5);
    let clock_before = controller.shared().clock_frames();
    assert!(
        clock_before > 0,
        "playback must be underway before the loss"
    );

    controller.backend_mut().remove_device(&lost_id);
    controller.tick();

    assert!(
        controller.is_connected(),
        "must fall back rather than go silent"
    );
    assert_eq!(
        controller.active_device().map(|d| d.is_fallback),
        Some(true)
    );
    assert_eq!(
        controller.preferred_device(),
        Some(&lost_id),
        "the preference is not overwritten by a mid-session fallback"
    );
    assert!(
        controller.shared().clock_frames() >= clock_before,
        "the clock must not reset on fallback"
    );

    let notification = controller.notifications().visible().next();
    assert_eq!(notification.map(|n| n.severity), Some(Severity::Critical));
    assert_eq!(notification.map(|n| n.message_key), Some("device-lost"));
    assert_eq!(
        notification
            .and_then(|n| n.args.first())
            .map(|(_, value)| value.as_str()),
        Some("USB Interface"),
        "the notification must name the device that was lost"
    );

    // Playback keeps working on the fallback device.
    let before = controller.shared().clock_frames();
    controller.backend_mut().render_buffers(5);
    assert!(controller.shared().clock_frames() > before);
}

/// US3 acceptance 3: the active device disappears and no output device
/// remains — transport pauses with position retained, a Critical
/// notification is raised, and nothing panics.
#[test]
fn pause_when_no_device_remains() {
    let (store, _dir) = fresh_store();
    let only_id = DeviceId::new("dev-1").unwrap_or_else(|| unreachable!());
    let devices = vec![fake_device("dev-1", "Only Output", true)];
    let mut controller =
        PlaybackController::new(FakeBackend::new(devices), SyntheticHost::new(44_100), store);
    controller.launch();
    controller.confirm_device(only_id.clone(), BufferPreset::Balanced);
    controller.queue_replace(vec![fixture_track()]);
    controller.play();
    controller.backend_mut().render_buffers(3);
    let position_before = controller.shared().position_frames();

    controller.backend_mut().remove_device(&only_id);
    controller.tick();

    assert!(!controller.is_connected());
    assert!(!controller.transport_available());
    assert_eq!(controller.transport(), Transport::Paused);
    assert_eq!(
        controller.shared().position_frames(),
        position_before,
        "position is retained, not reset, when no device remains"
    );

    let notification = controller.notifications().visible().next();
    assert_eq!(notification.map(|n| n.severity), Some(Severity::Critical));
    assert_eq!(
        notification.map(|n| n.message_key),
        Some("no-output-devices")
    );
}

/// US3 acceptance 4: after falling back, the lost device reappearing does
/// not switch playback back to it — the app stays on the fallback, raises
/// an Info notification once, and keeps the original preference.
#[test]
fn no_switch_back_on_reappear() {
    let (store, _dir) = fresh_store();
    let lost_id = DeviceId::new("dev-1").unwrap_or_else(|| unreachable!());
    let fallback_id = DeviceId::new("dev-2").unwrap_or_else(|| unreachable!());
    let devices = vec![
        fake_device("dev-1", "USB Interface", true),
        fake_device("dev-2", "Built-in Speakers", false),
    ];
    let mut controller =
        PlaybackController::new(FakeBackend::new(devices), SyntheticHost::new(44_100), store);
    controller.launch();
    controller.confirm_device(lost_id.clone(), BufferPreset::Balanced);

    controller.backend_mut().remove_device(&lost_id);
    controller.tick();
    assert_eq!(
        controller.active_device().map(|d| d.id.clone()),
        Some(fallback_id.clone())
    );

    // The lost device comes back.
    controller
        .backend_mut()
        .add_device(fake_device("dev-1", "USB Interface", false));
    controller.tick();

    assert_eq!(
        controller.active_device().map(|d| d.id.clone()),
        Some(fallback_id.clone()),
        "must never auto switch back to a reappeared device"
    );
    assert_eq!(controller.preferred_device(), Some(&lost_id));

    let info_notification = controller
        .notifications()
        .visible()
        .find(|n| n.message_key == "device-available-again");
    assert!(info_notification.is_some());
    assert_eq!(info_notification.map(|n| n.severity), Some(Severity::Info));
    assert_eq!(
        info_notification
            .and_then(|n| n.args.first())
            .map(|(_, value)| value.as_str()),
        Some("USB Interface")
    );

    let count_after_first_reappearance = controller
        .notifications()
        .all()
        .filter(|n| n.message_key == "device-available-again")
        .count();
    assert_eq!(count_after_first_reappearance, 1);

    // An unrelated later list change must not raise a duplicate.
    controller
        .backend_mut()
        .add_device(fake_device("dev-3", "Another Device", false));
    controller.tick();
    let count_after_unrelated_change = controller
        .notifications()
        .all()
        .filter(|n| n.message_key == "device-available-again")
        .count();
    assert_eq!(
        count_after_unrelated_change, 1,
        "must not re-notify on an unrelated list change"
    );
}

/// US3 acceptance 2 / FR-013: an external sample-rate change on the active
/// device rebuilds only the output stage — the clock and position are
/// unaffected and playback continues on the same device.
#[test]
fn rate_change_rebuilds_output_stage_only() {
    let (store, _dir) = fresh_store();
    let dev_id = DeviceId::new("dev-1").unwrap_or_else(|| unreachable!());
    let devices = vec![fake_device("dev-1", "Interface", true)];
    let mut controller =
        PlaybackController::new(FakeBackend::new(devices), SyntheticHost::new(44_100), store);
    controller.launch();
    controller.confirm_device(dev_id.clone(), BufferPreset::Balanced);
    controller.queue_replace(vec![fixture_track()]);
    controller.play();
    controller.backend_mut().render_buffers(5);
    let clock_before = controller.shared().clock_frames();
    let position_before = controller.shared().position_frames();
    assert!(clock_before > 0);

    controller
        .backend_mut()
        .set_rate(&dev_id, SampleRate::new(48_000));
    controller.tick();

    assert_eq!(
        controller.active_device().map(|d| d.id.clone()),
        Some(dev_id.clone()),
        "a rate change must not move playback to a different device"
    );
    assert_eq!(
        controller.active_device().map(|d| d.negotiated.device_rate),
        Some(SampleRate::new(48_000))
    );
    assert!(
        controller.shared().clock_frames() >= clock_before,
        "the clock is unaffected by a sample-rate change"
    );
    assert!(
        controller.shared().position_frames() >= position_before,
        "position is unaffected by a sample-rate change"
    );
    assert_eq!(
        controller.transport(),
        Transport::Playing,
        "transport shadow state carries through the rebuild"
    );

    // Playback keeps advancing on the rebuilt output stage.
    let before = controller.shared().clock_frames();
    controller.backend_mut().render_buffers(5);
    assert!(controller.shared().clock_frames() > before);
}

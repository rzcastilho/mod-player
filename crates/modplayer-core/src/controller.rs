// SPDX-License-Identifier: MIT OR Apache-2.0

//! `PlaybackController`: the single authority for transport and device
//! policy (data-model.md §5.2, AR-6). Every state change updates its
//! shadow state first, then is enqueued to a running `Processor`; on a
//! stream rebuild a fresh `Processor` is built from that shadow state plus
//! `RtShared::position_frames` (research R3). `shared` is created once at
//! construction and never replaced, so the clock survives every rebuild.
//!
//! `launch()` resolves and opens a stream per data-model.md §6.2's device
//! state machine (device_policy.rs, T048), auto-playing the test tone once
//! when the Device Check screen would be shown (T049). The effective
//! master volume at launch is `SafeVolume::apply(stored)` (US2 T061); the
//! continuous controls (`set_master_volume`, `set_ceiling`, transport) push
//! through a small pending-command buffer that retries a momentarily full
//! queue on the next `tick()` rather than dropping it (US2 T062,
//! contracts/engine-commands.md rule 3). Device-loss handling
//! (T071-T074) is a later phase.

use std::collections::VecDeque;
use std::sync::Arc;

use modplayer_audio_io::{BackendEvent, OpenStream, OutputBackend, OutputDeviceInfo};
use modplayer_audio_source::AudioSource;
use modplayer_audio_source_synthetic::SyntheticSource;
use modplayer_engine::{
    BufferPreset, CeilingDb, Command, DeviceId, Event, NegotiatedBuffer, Processor,
    ProcessorConfig, RtShared, SampleRate, Theme, Transport, VolumePercent,
};
use rtrb::{Consumer, Producer, RingBuffer};

use crate::device_policy::{self, DeviceLostOutcome, DeviceResolution, DeviceWarning};
use crate::notifications::{
    KEY_DEVICE_APPEARED, KEY_DEVICE_AVAILABLE_AGAIN, KEY_DEVICE_LOST, KEY_DEVICE_MISSING_AT_LAUNCH,
    KEY_NO_OUTPUT_DEVICES, NotificationCenter, Severity,
};
use crate::settings::{AudioSettings, SettingsStore, SettingsWarning};

/// Capacity of the command/event SPSC queues opened for each stream
/// (contracts/engine-commands.md).
const QUEUE_CAPACITY: usize = 256;

/// The active output device, if any (data-model.md §5.2).
pub struct ActiveDevice {
    pub id: DeviceId,
    pub name: String,
    pub negotiated: NegotiatedBuffer,
    pub is_fallback: bool,
    /// Whether the "device available again" Info notification has already
    /// been raised for this fallback (US3 acceptance 4). Internal
    /// bookkeeping only — reset by rebuilding a fresh `ActiveDevice` on the
    /// next device change.
    reappearance_notified: bool,
}

/// Single authority for playback shadow state, built on top of an injected
/// `OutputBackend` (`CpalBackend` in production, `FakeBackend` in tests).
pub struct PlaybackController<B: OutputBackend> {
    backend: B,

    transport: Transport,
    master_volume: VolumePercent,
    ceiling: CeilingDb,
    preset: BufferPreset,
    /// Mirrors `settings.theme` (FR-017). Read-only this phase — applied
    /// once at app startup (`modplayer-ui`'s `App::new`, T078/T081); the
    /// Appearance settings screen that changes it lands in US5.
    theme: Theme,
    active_device: Option<ActiveDevice>,
    /// Mirrors `settings.output_device`; never overwritten by a fallback
    /// (data-model.md §5.2).
    preferred_device: Option<DeviceId>,
    /// Mirrors `settings.device_confirmed` (FR-002-FR-004): `false` means
    /// the Device Check screen must be shown before normal use.
    device_confirmed: bool,
    stream: Option<OpenStream>,
    command_tx: Option<Producer<Command>>,
    event_rx: Option<Consumer<Event>>,
    /// Commands that failed to enqueue because the SPSC queue was
    /// momentarily full (or no stream was open yet), in order, retried by
    /// `flush_pending_commands` on the next `tick()`/stream open rather
    /// than dropped (contracts/engine-commands.md rule 3).
    pending_commands: VecDeque<Command>,
    /// Created once at construction; never replaced, so the audio clock
    /// survives every stream rebuild.
    shared: Arc<RtShared>,

    settings_store: SettingsStore,
    notifications: NotificationCenter,
}

impl<B: OutputBackend> PlaybackController<B> {
    /// Construct a controller wired to `backend`, seeding shadow state
    /// from `settings_store` (any load warning is raised as a
    /// notification). No stream is opened yet.
    pub fn new(backend: B, settings_store: SettingsStore) -> Self {
        let mut notifications = NotificationCenter::new();
        let outcome = settings_store.load();
        if let Some(warning) = outcome.warning {
            notifications.raise(severity_for(&warning), warning.message_key());
        }
        Self::from_settings(backend, settings_store, &outcome.settings, notifications)
    }

    fn from_settings(
        backend: B,
        settings_store: SettingsStore,
        settings: &AudioSettings,
        notifications: NotificationCenter,
    ) -> Self {
        Self {
            backend,
            // App launch is always Stopped, regardless of persisted state (FR-015).
            transport: Transport::Stopped,
            // Safe-volume startup clamp (US2 T061, FR-011): the effective
            // volume at launch is `SafeVolume::apply(stored)`; the stored
            // (possibly uncapped) value only comes back when the user
            // raises it again and it is persisted, re-clamped next launch.
            master_volume: settings.safe_volume.apply(settings.master_volume),
            ceiling: settings.limiter_ceiling_db,
            preset: settings.buffer_preset,
            theme: settings.theme,
            active_device: None,
            preferred_device: settings.output_device.clone(),
            device_confirmed: settings.device_confirmed,
            stream: None,
            command_tx: None,
            event_rx: None,
            pending_commands: VecDeque::new(),
            shared: Arc::new(RtShared::new()),
            settings_store,
            notifications,
        }
    }

    /// The shared real-time atomics — created once, never replaced.
    pub fn shared(&self) -> &Arc<RtShared> {
        &self.shared
    }

    /// Current transport shadow state.
    pub fn transport(&self) -> Transport {
        self.transport
    }

    /// Current master-volume shadow state.
    pub fn master_volume(&self) -> VolumePercent {
        self.master_volume
    }

    /// Current limiter-ceiling shadow state.
    pub fn ceiling(&self) -> CeilingDb {
        self.ceiling
    }

    /// Current buffer-preset shadow state.
    pub fn preset(&self) -> BufferPreset {
        self.preset
    }

    /// Current theme-preference shadow state (FR-017), seeded from
    /// settings at construction.
    pub fn theme(&self) -> Theme {
        self.theme
    }

    /// The device the user has confirmed/selected, if any (never
    /// overwritten by a fallback).
    pub fn preferred_device(&self) -> Option<&DeviceId> {
        self.preferred_device.as_ref()
    }

    /// The currently active device, if a stream is open.
    pub fn active_device(&self) -> Option<&ActiveDevice> {
        self.active_device.as_ref()
    }

    /// Whether a stream is currently open.
    pub fn is_connected(&self) -> bool {
        self.stream.is_some()
    }

    /// The notification center backing this controller's warnings/errors.
    pub fn notifications(&self) -> &NotificationCenter {
        &self.notifications
    }

    /// Mutable access to the notification center (e.g. for `tick`/`dismiss`).
    pub fn notifications_mut(&mut self) -> &mut NotificationCenter {
        &mut self.notifications
    }

    /// The backend this controller drives.
    pub fn backend(&self) -> &B {
        &self.backend
    }

    /// Mutable access to the backend this controller drives (device
    /// resolution and stream lifecycle, wired up in US1/US3).
    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }

    /// The settings store this controller loaded from and will save to.
    pub fn settings_store(&self) -> &SettingsStore {
        &self.settings_store
    }

    /// Whether the user has confirmed a device (Device Check "Yes").
    pub fn device_confirmed(&self) -> bool {
        self.device_confirmed
    }

    /// Whether transport controls should be enabled: `false` only in the
    /// zero-devices `NoDevice` state (FR-012, US1 edge case).
    pub fn transport_available(&self) -> bool {
        self.active_device.is_some()
    }

    /// Whether the Device Check screen must be shown: no device confirmed
    /// yet, or there are zero devices to preview (data-model.md §6.2,
    /// ui-surface.md's empty state).
    pub fn should_show_device_check(&self) -> bool {
        !self.device_confirmed || self.active_device.is_none()
    }

    /// Resolve and open the initial stream per data-model.md §6.2's device
    /// state machine, raising any warning/critical notification the
    /// resolution implies. When no device has been confirmed yet, the test
    /// tone auto-plays once on the previewed default device (FR-003).
    pub fn launch(&mut self) {
        let devices = self.backend.devices().unwrap_or_default();
        let (resolution, warning) = device_policy::resolve(
            &devices,
            self.preferred_device.as_ref(),
            self.device_confirmed,
        );
        self.raise_device_warning(warning);

        match resolution {
            DeviceResolution::NoDevice => self.disconnect(),
            DeviceResolution::Active {
                device,
                is_fallback,
            } => {
                self.open_stream_on(device, is_fallback);
                if !self.device_confirmed {
                    self.play_test_tone();
                }
            }
        }
    }

    /// Preview a specific device: reopens the stream on it and restarts the
    /// test tone (Device Check's device-select / "No, try another"). A
    /// no-op if `device` is no longer enumerable.
    pub fn preview_device(&mut self, device: DeviceId) {
        let devices = self.backend.devices().unwrap_or_default();
        if let Some(info) = devices.into_iter().find(|d| d.id == device) {
            self.open_stream_on(info, false);
            self.play_test_tone();
        }
    }

    /// (Re)start the test tone on the currently open stream. Returns `true`
    /// if a stream was open to send the command to (US1 edge case: zero
    /// devices attempts no tone).
    pub fn play_test_tone(&mut self) -> bool {
        self.push_command(Command::PlayTestTone)
    }

    /// Update master-volume shadow state, enqueue `SetMasterVolume`
    /// (retrying a momentarily full queue rather than dropping it), and
    /// persist the new value so it survives restart, re-clamped by the
    /// safe-volume cap on the next launch (spec US2 acceptance 2-4;
    /// contracts/engine-commands.md rule 3). Debouncing rapid writes
    /// (plan.md design note 5) is deferred; each call saves synchronously.
    pub fn set_master_volume(&mut self, volume: VolumePercent) {
        self.master_volume = volume;
        self.push_command_retrying(Command::SetMasterVolume(volume));
        self.persist_settings(|settings| settings.master_volume = volume);
    }

    /// Update limiter-ceiling shadow state, enqueue `SetCeiling` (retrying
    /// a momentarily full queue), and persist the new value (spec US2
    /// acceptance 1, 4).
    pub fn set_ceiling(&mut self, ceiling: CeilingDb) {
        self.ceiling = ceiling;
        self.push_command_retrying(Command::SetCeiling(ceiling));
        self.persist_settings(|settings| settings.limiter_ceiling_db = ceiling);
    }

    /// Transition transport shadow state to `Playing` and enqueue `Play`
    /// (spec US2 acceptance 6; contracts/engine-commands.md).
    pub fn play(&mut self) {
        self.transport = Transport::Playing;
        self.push_command_retrying(Command::Play);
    }

    /// Transition transport shadow state to `Paused` and enqueue `Pause`.
    pub fn pause(&mut self) {
        self.transport = Transport::Paused;
        self.push_command_retrying(Command::Pause);
    }

    /// Transition transport shadow state to `Stopped` and enqueue `Stop`.
    pub fn stop(&mut self) {
        self.transport = Transport::Stopped;
        self.push_command_retrying(Command::Stop);
    }

    /// Retry any commands that failed to enqueue on a previous attempt
    /// because the SPSC queue was momentarily full (contracts/engine-
    /// commands.md rule 3: the controller "never blocks and never drops
    /// the shadow update"), and drain any device events the backend has
    /// raised (device loss, list changes, sample-rate changes — US3
    /// T069-T074). Call once per UI tick.
    pub fn tick(&mut self) {
        self.flush_pending_commands();
        self.drain_backend_events();
    }

    /// Device Check "Yes": persist `{output_device, buffer_preset,
    /// device_confirmed = true}` and make `device` the active, non-fallback
    /// stream (contracts data-model.md §6.3).
    pub fn confirm_device(&mut self, device: DeviceId, preset: BufferPreset) {
        self.preset = preset;
        self.preferred_device = Some(device.clone());
        self.device_confirmed = true;

        let mut settings = self.settings_store.load().settings;
        settings.output_device = Some(device.clone());
        settings.buffer_preset = preset;
        settings.device_confirmed = true;
        if self.settings_store.save(&settings).is_err() {
            self.notifications
                .raise(Severity::Warning, "settings-save-failed");
        }

        let devices = self.backend.devices().unwrap_or_default();
        if let Some(info) = devices.into_iter().find(|d| d.id == device) {
            self.open_stream_on(info, false);
        }
    }

    /// Device Check "Skip for now": `device_confirmed` stays `false` and
    /// nothing is persisted, so the screen reappears next launch
    /// (data-model.md §6.3). The already-previewed default device, if any,
    /// stays active so playback still works this session.
    pub fn skip_device_check(&mut self) {
        // Intentionally a no-op on shadow/persisted state — see doc comment.
    }

    fn raise_device_warning(&mut self, warning: Option<DeviceWarning>) {
        match warning {
            None => {}
            Some(DeviceWarning::NoDevices) => {
                self.notifications
                    .raise(Severity::Critical, KEY_NO_OUTPUT_DEVICES);
            }
            Some(DeviceWarning::MissingPreferred { device_name }) => {
                self.notifications.raise_with_args(
                    Severity::Warning,
                    KEY_DEVICE_MISSING_AT_LAUNCH,
                    vec![("device", device_name)],
                );
            }
        }
    }

    /// Drain every `BackendEvent` the backend has queued and react to each
    /// (US3 T069-T074). Collected into a local `Vec` first so the borrow of
    /// `self.backend` (via `events()`) ends before any handler needs
    /// `&mut self` (e.g. to raise a notification or rebuild the stream).
    fn drain_backend_events(&mut self) {
        let mut events = Vec::new();
        while let Ok(event) = self.backend.events().try_recv() {
            events.push(event);
        }
        for event in events {
            self.handle_backend_event(event);
        }
    }

    fn handle_backend_event(&mut self, event: BackendEvent) {
        match event {
            BackendEvent::DeviceLost { id } => self.handle_device_lost(id),
            BackendEvent::DeviceListChanged => self.handle_device_list_changed(),
            BackendEvent::SampleRateChanged { id, new_rate } => {
                self.handle_sample_rate_changed(id, new_rate)
            }
        }
    }

    /// The active stream's device disappeared (US3 acceptance 1, 3;
    /// data-model.md §6.2). Falls back to the system default within the
    /// current tick — `open_stream_on` is the same snapshot-rebuild path
    /// `launch()`/`confirm_device()` use, so the clock (`Arc<RtShared>`,
    /// never replaced) and position carry forward unchanged (research R3).
    /// If no device remains at all, transport pauses with position
    /// retained rather than reset.
    fn handle_device_lost(&mut self, id: DeviceId) {
        let Some(active) = self.active_device.as_ref() else {
            return;
        };
        if active.id != id {
            // Stale event for a device we already moved on from.
            return;
        }
        let lost_device_name = active.name.clone();

        let devices = self.backend.devices().unwrap_or_default();
        match device_policy::on_device_lost(&devices, lost_device_name) {
            DeviceLostOutcome::FellBackTo {
                device,
                lost_device_name,
            } => {
                self.open_stream_on(device, true);
                self.notifications.raise_with_args(
                    Severity::Critical,
                    KEY_DEVICE_LOST,
                    vec![("device", lost_device_name)],
                );
            }
            DeviceLostOutcome::NoDeviceRemains => {
                self.disconnect();
                // Position is retained: it lives in `self.shared`, which is
                // never reset by `disconnect()`.
                self.transport = Transport::Paused;
                self.notifications
                    .raise(Severity::Critical, KEY_NO_OUTPUT_DEVICES);
            }
        }
    }

    /// Any device add/remove (US3 acceptance 4; US1 edge case "device
    /// appears after zero-devices"). Two independent cases, neither of
    /// which ever switches an already-open stream to a different device
    /// (data-model.md §6.2: "never auto switch back"):
    ///
    /// - On a fallback stream, if the preferred device has reappeared,
    ///   raise an Info notification once and keep playing on the fallback.
    /// - In the zero-device (`NoDevice`) state, if a device now exists,
    ///   open a stream on it (same resolution `launch()` uses) and raise an
    ///   Info notification that a device appeared.
    fn handle_device_list_changed(&mut self) {
        let devices = self.backend.devices().unwrap_or_default();

        if let Some(active) = self.active_device.as_mut() {
            if active.is_fallback
                && !active.reappearance_notified
                && let Some(preferred) = self.preferred_device.as_ref()
                && let Some(found) = device_policy::reappeared(&devices, preferred)
            {
                active.reappearance_notified = true;
                self.notifications.raise_with_args(
                    Severity::Info,
                    KEY_DEVICE_AVAILABLE_AGAIN,
                    vec![("device", found.name.clone())],
                );
            }
            return;
        }

        if devices.is_empty() {
            return;
        }
        let (resolution, _warning) = device_policy::resolve(
            &devices,
            self.preferred_device.as_ref(),
            self.device_confirmed,
        );
        if let DeviceResolution::Active {
            device,
            is_fallback,
        } = resolution
        {
            self.open_stream_on(device, is_fallback);
            self.notifications
                .raise(Severity::Info, KEY_DEVICE_APPEARED);
        }
    }

    /// The active device's sample rate changed externally (US3 acceptance
    /// 2, FR-013): rebuild the stream on the same device so the output
    /// stage picks up the new device rate. `open_stream_on` is the
    /// snapshot-rebuild path — the source runs at its own fixed rate
    /// throughout, and the clock/position carry forward via `self.shared`,
    /// so only the output stage's resampling target actually changes.
    fn handle_sample_rate_changed(&mut self, id: DeviceId, _new_rate: SampleRate) {
        let Some(active) = self.active_device.as_ref() else {
            return;
        };
        if active.id != id {
            return;
        }
        let is_fallback = active.is_fallback;

        let devices = self.backend.devices().unwrap_or_default();
        if let Some(info) = devices.into_iter().find(|d| d.id == id) {
            self.open_stream_on(info, is_fallback);
        }
    }

    /// Build a fresh `Processor` for `device` and open a stream on it,
    /// carrying position/clock forward from `self.shared` (research R3).
    fn open_stream_on(&mut self, device: OutputDeviceInfo, is_fallback: bool) {
        // Stop the outgoing stream *before* snapshotting position and
        // opening the new one: otherwise both callbacks run concurrently
        // for the duration of `open` (the OS mixes them — a brief doubled,
        // phase-smeared burst) and the new processor resumes from a
        // position the old one has already moved past. On failure below
        // `disconnect` clears the rest of the shadow state anyway.
        self.stream = None;
        let (command_tx, command_rx) = RingBuffer::<Command>::new(QUEUE_CAPACITY);
        let (event_tx, event_rx) = RingBuffer::<Event>::new(QUEUE_CAPACITY);
        let source = SyntheticSource::default();
        let config = ProcessorConfig {
            source_rate: source.sample_rate(),
            device_rate: device.default_rate.hz(),
            device_channels: device.channels,
            max_frames: self.preset.requested_frames().frames() as usize,
            transport: self.transport,
            position_frames: self.shared.position_frames(),
            master_volume: self.master_volume,
            ceiling: self.ceiling,
            shared: Arc::clone(&self.shared),
        };
        let processor = Processor::new(config, source, command_rx, event_tx);

        match self.backend.open(&device.id, self.preset, processor) {
            Ok(stream) => {
                self.active_device = Some(ActiveDevice {
                    id: device.id,
                    name: device.name,
                    negotiated: stream.negotiated,
                    is_fallback,
                    reappearance_notified: false,
                });
                self.stream = Some(stream);
                self.command_tx = Some(command_tx);
                self.event_rx = Some(event_rx);
                // Flush anything queued while no stream was open (or a
                // prior stream's queue was full), now that a fresh queue
                // exists (contracts/engine-commands.md rule 3).
                self.flush_pending_commands();
            }
            Err(_) => self.disconnect(),
        }
    }

    /// Drop any open stream and clear device shadow state (`NoDevice`).
    fn disconnect(&mut self) {
        self.active_device = None;
        self.stream = None;
        self.command_tx = None;
        self.event_rx = None;
    }

    /// Push `command` to the running processor, if any. A single
    /// non-blocking attempt: on a full queue the command is dropped rather
    /// than retried here (the bounded-retry producer for continuous
    /// controls lands in US2 T062). Returns whether a stream was open to
    /// send to.
    fn push_command(&mut self, command: Command) -> bool {
        match self.command_tx.as_mut() {
            Some(tx) => tx.push(command).is_ok(),
            None => false,
        }
    }

    /// Push `command` to the running processor, retrying on a subsequent
    /// `tick()` (or the next stream open) if the queue is momentarily full
    /// or no stream is open yet, instead of dropping it
    /// (contracts/engine-commands.md rule 3). Used by the continuous
    /// controls (`set_master_volume`, `set_ceiling`, transport).
    fn push_command_retrying(&mut self, command: Command) {
        self.pending_commands.push_back(command);
        self.flush_pending_commands();
    }

    /// Drain `pending_commands` into the running processor's queue, in
    /// order, stopping at the first command that still doesn't fit (it and
    /// everything behind it retry on the next call). A no-op when no
    /// stream is open.
    fn flush_pending_commands(&mut self) {
        let Some(tx) = self.command_tx.as_mut() else {
            return;
        };
        while let Some(&command) = self.pending_commands.front() {
            if tx.push(command).is_ok() {
                self.pending_commands.pop_front();
            } else {
                break;
            }
        }
    }

    /// Load the current settings, apply `mutate`, and save — reporting a
    /// `settings-save-failed` warning on failure, matching `confirm_device`
    /// (US1 T049).
    fn persist_settings(&mut self, mutate: impl FnOnce(&mut AudioSettings)) {
        let mut settings = self.settings_store.load().settings;
        mutate(&mut settings);
        if self.settings_store.save(&settings).is_err() {
            self.notifications
                .raise(Severity::Warning, "settings-save-failed");
        }
    }
}

fn severity_for(warning: &SettingsWarning) -> Severity {
    match warning {
        SettingsWarning::Unreadable
        | SettingsWarning::NewerVersion
        | SettingsWarning::InvalidValue(_) => Severity::Warning,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use modplayer_audio_io::FakeBackend;

    fn fresh_store() -> SettingsStore {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-controller-test-{}-{}",
            std::process::id(),
            unique
        ));
        let _ = std::fs::create_dir_all(&dir);
        SettingsStore::with_path(dir.join("settings.toml"))
    }

    #[test]
    fn transport_always_starts_stopped() {
        let controller = PlaybackController::new(FakeBackend::new(vec![]), fresh_store());
        assert_eq!(controller.transport(), Transport::Stopped);
    }

    #[test]
    fn shadow_state_seeded_from_defaults_with_no_settings_file() {
        let controller = PlaybackController::new(FakeBackend::new(vec![]), fresh_store());
        // Default settings: master volume 80%, safe volume enabled with a
        // 50% cap — so the effective launch volume is the cap (US2 T061,
        // `crates/modplayer-core/tests/safe_volume.rs` covers the clamp
        // itself in detail).
        assert_eq!(controller.master_volume(), VolumePercent::new(50));
        assert_eq!(controller.ceiling(), CeilingDb::default());
        assert_eq!(controller.preset(), BufferPreset::Balanced);
        assert_eq!(controller.preferred_device(), None);
        assert!(!controller.is_connected());
        assert_eq!(controller.shared().clock_frames(), 0);
        assert_eq!(controller.notifications().visible().count(), 0);
    }

    #[test]
    fn shared_atomics_are_created_once_and_reachable() {
        let controller = PlaybackController::new(FakeBackend::new(vec![]), fresh_store());
        let shared = controller.shared();
        shared.advance_clock(42);
        assert_eq!(controller.shared().clock_frames(), 42);
    }

    #[test]
    fn a_settings_load_warning_becomes_a_notification() {
        let store = fresh_store();
        let _ = std::fs::write(store.path(), "not valid toml {{{");
        let controller = PlaybackController::new(FakeBackend::new(vec![]), store);
        assert_eq!(controller.notifications().visible().count(), 1);
        let notification = controller.notifications().visible().next();
        assert_eq!(notification.map(|n| n.severity), Some(Severity::Warning));
        assert_eq!(
            notification.map(|n| n.message_key),
            Some("settings-unreadable")
        );
    }
}

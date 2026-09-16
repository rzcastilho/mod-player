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
use std::time::{Duration, Instant};

use modplayer_audio_io::{BackendEvent, OpenStream, OutputBackend, OutputDeviceInfo};
use modplayer_audio_source::{
    AccountReadError, AudioSource, Program, Repeat, SourceCommand, SourceEvent, SourceHealth,
    SourceHost, TrackId, TrackRef,
};
use modplayer_engine::{
    BufferPreset, CeilingDb, Command, DeviceId, Event, NegotiatedBuffer, PositionClock, Processor,
    ProcessorConfig, RtShared, SampleRate, Theme, Transport, VolumePercent,
};
use rtrb::{Consumer, Producer, RingBuffer};

use crate::device_policy::{self, DeviceLostOutcome, DeviceResolution, DeviceWarning};
use crate::notifications::{
    KEY_DEVICE_APPEARED, KEY_DEVICE_AVAILABLE_AGAIN, KEY_DEVICE_LOST, KEY_DEVICE_MISSING_AT_LAUNCH,
    KEY_NO_OUTPUT_DEVICES, KEY_QUEUE_ITEM_SKIPPED_UNAVAILABLE, NotificationCenter, Severity,
};
use crate::queue::{
    AdvanceReason, Origin, PlaybackChange, Queue, QueueChange, QueueItem, QueueItemId, QueueMode,
    XorShiftRng,
};
use crate::settings::{
    AudioSettings, DeviceName, DeviceNameError, SettingsStore, SettingsWarning,
    generate_connect_device_id,
};
use crate::transport::{
    self, ActiveState, Effect, Input, Intent, NotRegisteredReason, PendingTransferCommand,
    QueueChangeOrigin, QueueOp, TimerCommand, TimerKind, TransportState,
};

/// Capacity of the command/event SPSC queues opened for each stream
/// (contracts/engine-commands.md).
const QUEUE_CAPACITY: usize = 256;

/// `sync_program`'s debounce window (contracts/transport-and-queue.md §3):
/// at most one `LoadProgram` send per this interval, the last mutation
/// before it elapses wins.
const PROGRAM_SEND_DEBOUNCE: Duration = Duration::from_millis(250);

/// The "take over playback here" transfer request timeout (contracts/
/// transport-and-queue.md §2 rule T19, design note 6).
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(5);

/// The transient-health reconnect-warning timeout (contracts/transport-
/// and-queue.md §2 rule T20, US4, design note 6).
const RECONNECT_WARNING_TIMEOUT: Duration = Duration::from_secs(30);

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

/// One row of `queue_view()`'s projection (contracts/transport-and-
/// queue.md §1, contracts/ui-surface.md §2).
#[derive(Debug, Clone, PartialEq)]
pub struct QueueRow {
    pub uid: QueueItemId,
    pub title: String,
    pub artist: String,
    pub origin: Origin,
    pub unavailable: bool,
    pub is_current: bool,
}

/// The Queue panel's projection of `Queue` (T066): the effective order,
/// current item first, history excluded, plus the mode toggles it also
/// needs to render (contracts/transport-and-queue.md §1).
#[derive(Debug, Clone, PartialEq)]
pub struct QueueView {
    pub items: Vec<QueueRow>,
    pub shuffle: bool,
    pub repeat: Repeat,
}

/// A `sync_program`-computed send waiting out the debounce window
/// (contracts/transport-and-queue.md §3). Everything but `generation`
/// (assigned only once the send actually happens, in `flush_pending_
/// program`) is fixed at the moment `sync_program` last recomputed it.
#[derive(Debug, Clone, PartialEq)]
struct PendingProgram {
    order: Vec<TrackId>,
    cursor_index: u32,
    position_ms: u32,
    start_playing: bool,
    repeat_all: bool,
    repeat_one: bool,
}

/// Single authority for playback shadow state, built on top of an injected
/// `OutputBackend` (`CpalBackend` in production, `FakeBackend` in tests)
/// and an injected `SourceHost` (`SyntheticHost` in production before a
/// real source is wired up, `ScriptedHost` in tests, `ConnectSource` from
/// US1 on — research R10). `Processor<H::Rt>` stays monomorphic (no trait
/// object on the real-time path); only this controller and `open_stream_on`
/// know about `H`.
pub struct PlaybackController<B: OutputBackend, H: SourceHost> {
    backend: B,
    source_host: H,

    /// The host-owned play queue (data-model.md §2.2) and transport
    /// reducer state (data-model.md §3.1). Wiring `play()`/`pause()`/etc.
    /// through `transport::reduce` against these lands with US1 (T047);
    /// this phase holds them and drives `tick()`'s source-event mirror
    /// (T11/T12) through the reducer already.
    queue: Queue,
    transport_state: TransportState,
    queue_rng: XorShiftRng,
    /// Monotonically increasing program-send generation (contracts/
    /// transport-and-queue.md §3).
    program_generation: u64,
    /// The `(order, cursor_index, repeat_all, repeat_one)` of the last
    /// `Program` `sync_program` actually sent, so a mutation that leaves
    /// those fields unchanged sends nothing (contracts/transport-and-
    /// queue.md §3). `None` before the first send.
    last_sent_program_key: Option<(Vec<TrackId>, u32, bool, bool)>,
    /// A `sync_program`-computed send still waiting out the debounce
    /// window (§3: "debounced to at most one send per 250 ms, the last
    /// wins") — overwritten, not queued, by every further `sync_program`
    /// call before it flushes.
    pending_program: Option<PendingProgram>,
    /// When `sync_program` last actually sent a `LoadProgram` — `None`
    /// before the first send, so that one always goes out immediately.
    last_program_sent_at: Option<Instant>,
    /// The current stream's source sample rate, captured at `attach()`
    /// time — needed to convert `Effect::SeekTo`'s milliseconds to frames.
    /// `0` before any stream has ever been opened.
    source_sample_rate: u32,
    /// Injectable wall clock (design note 6): `Instant::now` in
    /// production, a fixed/advancing fake in tests (`set_clock`), so the
    /// 5 s transfer timer (T19) and the 30 s transient timer (US4) land
    /// testable without sleeping.
    now: Arc<dyn Fn() -> Instant + Send + Sync>,
    /// The 5 s transfer-request timer's deadline (T17/T19), `None` unless
    /// `ActiveState::TransferRequested`. Checked every `tick()` against
    /// `self.now()` rather than a background thread, matching how
    /// `flush_pending_program`'s debounce is already polled.
    transfer_timer_deadline: Option<Instant>,
    /// The 30 s reconnect-warning timer's deadline (T20, US4), `None`
    /// unless `SourceHealth::Transient` is currently in effect. Checked
    /// every `tick()` the same way as `transfer_timer_deadline`.
    reconnect_timer_deadline: Option<Instant>,

    /// `[playback] device_name` shadow state (FR-001); `None` = use the
    /// default name (`device_name()` computes it).
    device_name: Option<DeviceName>,
    /// `[playback] connect_device_id` shadow state (research R8):
    /// generated once and persisted on the very first launch that needs
    /// one.
    device_id: String,
    /// Whether `Initialize` has been sent and not yet followed by a
    /// `Deregister` (`set_playback_permitted`'s de-dup guard).
    registered_or_pending: bool,
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
    /// Monotonic id for `request_account_tracks` so a reply can be matched
    /// to its request (Stage 2 "Play from account" over the session).
    next_account_read_id: u64,
    /// `SourceEvent::AccountTracks` replies drained from the source but not
    /// yet handed to the UI via `take_account_tracks`.
    pending_account_tracks: Vec<(u64, Result<Vec<TrackRef>, AccountReadError>)>,
}

impl<B: OutputBackend, H: SourceHost> PlaybackController<B, H> {
    /// Construct a controller wired to `backend` and `source_host`,
    /// seeding shadow state from `settings_store` (any load warning is
    /// raised as a notification). No stream is opened yet; `source_host`
    /// is not sent `Initialize` here (that is `set_playback_permitted`'s
    /// job, driven by account state).
    pub fn new(backend: B, source_host: H, settings_store: SettingsStore) -> Self {
        let mut notifications = NotificationCenter::new();
        let outcome = settings_store.load();
        if let Some(warning) = outcome.warning {
            notifications.raise(severity_for(&warning), warning.message_key());
        }
        let needs_device_id_persisted = outcome.settings.connect_device_id.is_none();
        let mut controller = Self::from_settings(
            backend,
            source_host,
            settings_store,
            &outcome.settings,
            notifications,
        );
        // First launch that ever needed a device id (research R8): persist
        // it now so every subsequent launch (and every other controller)
        // sees the same stable id.
        if needs_device_id_persisted {
            let device_id = controller.device_id.clone();
            controller.persist_settings(|settings| settings.connect_device_id = Some(device_id));
        }
        controller
    }

    fn from_settings(
        backend: B,
        source_host: H,
        settings_store: SettingsStore,
        settings: &AudioSettings,
        notifications: NotificationCenter,
    ) -> Self {
        Self {
            backend,
            source_host,
            queue: Queue::new(),
            transport_state: TransportState::default(),
            queue_rng: XorShiftRng::seeded(),
            program_generation: 0,
            last_sent_program_key: None,
            pending_program: None,
            last_program_sent_at: None,
            source_sample_rate: 0,
            now: Arc::new(Instant::now),
            transfer_timer_deadline: None,
            reconnect_timer_deadline: None,
            device_name: settings.device_name.clone(),
            device_id: settings
                .connect_device_id
                .clone()
                .unwrap_or_else(generate_connect_device_id),
            registered_or_pending: false,
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
            next_account_read_id: 0,
            pending_account_tracks: Vec::new(),
        }
    }

    /// Current transport-reducer shadow state (data-model.md §3.1).
    pub fn transport_state(&self) -> &TransportState {
        &self.transport_state
    }

    /// The host-owned play queue (data-model.md §2.2).
    pub fn queue(&self) -> &Queue {
        &self.queue
    }

    /// The injected wall clock's current reading (design note 6);
    /// `Instant::now()` in production. Timers that consult it (5 s
    /// transfer, 30 s transient) land with US3/US4.
    pub fn now(&self) -> Instant {
        (self.now)()
    }

    /// Replace the wall clock `tick()`'s timers (T19's 5 s transfer
    /// timeout) read against (design note 6) — production never calls
    /// this; tests inject a fake/advancing clock instead of sleeping.
    pub fn set_clock(&mut self, now: impl Fn() -> Instant + Send + Sync + 'static) {
        self.now = Arc::new(now);
    }

    /// The shared real-time atomics — created once, never replaced.
    pub fn shared(&self) -> &Arc<RtShared> {
        &self.shared
    }

    /// Current transport shadow state, derived from the reducer's
    /// `Intent` (data-model.md §3.1) — kept for callers that only need
    /// the coarse `Stopped`/`Playing`/`Paused` engine-facing value; new
    /// code should prefer `transport_state().intent`.
    pub fn transport(&self) -> Transport {
        self.engine_transport()
    }

    fn engine_transport(&self) -> Transport {
        match self.transport_state.intent {
            Intent::Stopped => Transport::Stopped,
            Intent::Playing => Transport::Playing,
            Intent::Paused => Transport::Paused,
        }
    }

    /// Connect device status (data-model.md §3.1, FR-016/018/019/027).
    pub fn active_state(&self) -> &ActiveState {
        &self.transport_state.active
    }

    /// Whether the transport controls should be enabled this frame
    /// (contracts/transport-and-queue.md §1): a device is open, the
    /// source is not in an unrecoverable state, and the device is
    /// currently registered with the service.
    pub fn transport_enabled(&self) -> bool {
        self.active_device.is_some()
            && !matches!(
                self.transport_state.health,
                SourceHealth::Unavailable { .. }
            )
            && !matches!(
                self.transport_state.active,
                ActiveState::NotRegistered { .. }
            )
    }

    /// The Fluent key for the inline reason transport is disabled, if any
    /// (contracts/transport-and-queue.md §1).
    pub fn disabled_reason(&self) -> Option<&'static str> {
        if self.active_device.is_none() {
            return Some("status-no-device");
        }
        if let ActiveState::NotRegistered { reason } = &self.transport_state.active {
            return Some(match reason {
                NotRegisteredReason::PremiumRequired => "status-premium-required",
                NotRegisteredReason::SubscriptionNotVerified => "status-subscription-not-verified",
                NotRegisteredReason::SignedOut => "status-signed-out",
                NotRegisteredReason::Unknown => "status-not-registered",
            });
        }
        if matches!(
            self.transport_state.health,
            SourceHealth::Unavailable { .. }
        ) {
            return Some("status-source-unavailable");
        }
        None
    }

    /// Current playback position, derived from the audio clock at >= 60 Hz
    /// (engine-delta.md §3, FR-006) — safe to call every frame. `Stopped`
    /// (T5) always reads zero: the engine only republishes its anchor on a
    /// *playing* render, so the shadow `intent` is authoritative for the
    /// stopped case rather than waiting for one more render to catch up.
    pub fn position(&self) -> Duration {
        if self.transport_state.intent == Intent::Stopped {
            return Duration::ZERO;
        }
        PositionClock::now(&self.shared, self.source_sample_rate.max(1))
    }

    /// The queue's current item, if any.
    pub fn current_track(&self) -> Option<&QueueItem> {
        self.queue.current()
    }

    /// The effective Connect device name: the user's custom name, or the
    /// service/platform default when none is set (FR-001).
    pub fn device_name(&self) -> String {
        match &self.device_name {
            Some(name) => name.as_str().to_string(),
            None => default_device_name(),
        }
    }

    /// Validate, persist, and (if registered) apply a new device name
    /// (contracts/transport-and-queue.md §5, FR-001). An empty/whitespace
    /// `input` restores the default.
    pub fn set_device_name(&mut self, input: &str) -> Result<(), DeviceNameError> {
        let parsed = DeviceName::parse(input)?;
        self.device_name = parsed.clone();
        self.persist_settings(|settings| settings.device_name = parsed.clone());
        if self.registered_or_pending {
            self.source_host
                .command(SourceCommand::SetDeviceName(self.device_name()));
        }
        Ok(())
    }

    /// Called by `App` from 002's session state (contracts/transport-and-
    /// queue.md §1): `true` sends `Initialize` once; `false` sends
    /// `Deregister` once and disables transport with `reason` inline.
    pub fn set_playback_permitted(&mut self, permitted: bool, reason: Option<NotRegisteredReason>) {
        if permitted {
            if !self.registered_or_pending {
                self.registered_or_pending = true;
                self.source_host.command(SourceCommand::Initialize {
                    device_name: self.device_name(),
                    device_id: self.device_id.clone(),
                });
            }
        } else {
            if self.registered_or_pending {
                self.registered_or_pending = false;
                self.source_host.command(SourceCommand::Deregister);
            }
            self.transport_state.active = ActiveState::NotRegistered {
                reason: reason.unwrap_or(NotRegisteredReason::Unknown),
            };
        }
    }

    /// Ask the source to tear down and re-run `Initialize` after
    /// `Unavailable` (contracts/audio-source-host.md §2).
    pub fn retry_source(&mut self) {
        self.source_host.command(SourceCommand::Retry);
    }

    /// Sign-out ordering (design note 7): stop, clear the queue, and
    /// deregister — called by `App` *before* it processes 002's
    /// `SignedOut`/`SessionRevoked` report.
    pub fn clear_for_sign_out(&mut self) {
        self.stop();
        self.queue = Queue::new();
        if self.registered_or_pending {
            self.registered_or_pending = false;
            self.source_host.command(SourceCommand::Deregister);
        }
        self.transport_state.active = ActiveState::NotRegistered {
            reason: NotRegisteredReason::SignedOut,
        };
    }

    /// Login refused for a non-Premium account (FR-027, rule T22): the
    /// current item, if any is playing, finishes before the device
    /// disables and deregisters (`transport::reduce`'s `downgrade_pending`
    /// bookkeeping) — immediate when nothing is currently playing.
    pub fn on_tier_rejected(&mut self) {
        self.dispatch(Input::TierRejected);
    }

    /// The account tier dropped to Free (FR-027). See `on_tier_rejected`'s
    /// doc comment — same finish-then-disable rule (T22).
    pub fn on_tier_free(&mut self) {
        self.on_tier_rejected();
    }

    /// `App::on_exit` (design note 8, FR-008): stop, release the source's
    /// resources, and drop the stream.
    pub fn shutdown(&mut self) {
        self.stop();
        self.source_host.command(SourceCommand::Shutdown);
        self.stream = None;
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
        if self.registered_or_pending {
            self.source_host.command(SourceCommand::SetVolume(
                modplayer_audio_source::VolumePercent::new(volume.value()),
            ));
        }
    }

    /// Update limiter-ceiling shadow state, enqueue `SetCeiling` (retrying
    /// a momentarily full queue), and persist the new value (spec US2
    /// acceptance 1, 4).
    pub fn set_ceiling(&mut self, ceiling: CeilingDb) {
        self.ceiling = ceiling;
        self.push_command_retrying(Command::SetCeiling(ceiling));
        self.persist_settings(|settings| settings.limiter_ceiling_db = ceiling);
    }

    /// `play` per the transport reducer's rules T1-T3
    /// (contracts/transport-and-queue.md §2): starts the current queue
    /// item (loading the program first if the source doesn't already
    /// have it loaded), resumes from pause, or is a no-op while buffering.
    pub fn play(&mut self) {
        let has_current = self.queue.current().is_some();
        let track_loaded_in_source = self.transport_state.track_len_ms.is_some();
        let buffer_ready = self.source_host.buffer_status().ready;
        self.dispatch(Input::Play {
            has_current,
            track_loaded_in_source,
            buffer_ready,
        });
    }

    /// `pause` per rule T4.
    pub fn pause(&mut self) {
        self.dispatch(Input::Pause);
    }

    /// `stop` per rule T5: position resets to 0; the current item, queue,
    /// and Connect active status are retained.
    pub fn stop(&mut self) {
        self.dispatch(Input::Stop);
    }

    /// `seek` per rules T6-T8: clamps at the track end (advancing per
    /// FR-014 rather than overshooting).
    pub fn seek(&mut self, position: Duration) {
        let position_ms = u32::try_from(position.as_millis()).unwrap_or(u32::MAX);
        let buffer_ready = self.source_host.buffer_status().ready;
        self.dispatch(Input::Seek {
            position_ms,
            buffer_ready,
        });
    }

    /// `skip_forward` per rule T9.
    pub fn skip_forward(&mut self) {
        self.dispatch(Input::SkipForward);
    }

    /// `skip_back` per rule T10: restarts the current item past the 3 s
    /// threshold (`Queue::skip_back`'s own rule), otherwise moves to the
    /// previous item.
    pub fn skip_back(&mut self) {
        let position_ms = u32::try_from(self.position().as_millis()).unwrap_or(0);
        self.dispatch(Input::SkipBack { position_ms });
    }

    /// **Play here** (contracts/transport-and-queue.md §1, FR-018): ask the
    /// service to make this device the active one. Only meaningful while
    /// `ActiveState::Inactive` (the banner that offers this button is only
    /// shown then); a no-op otherwise (rule T17).
    pub fn play_here(&mut self) {
        self.dispatch(Input::PlayHere);
    }

    /// The other Connect device's name, once known (T77, research R3):
    /// updates the `Inactive` banner's `other_device` field. A no-op
    /// unless the device is currently `Inactive` — a name that arrives
    /// after the device became active again (or before `BecameInactive`
    /// even landed) has nothing to attach to.
    pub fn set_other_device_name(&mut self, name: Option<String>) {
        if let ActiveState::Inactive { other_device } = &mut self.transport_state.active {
            *other_device = name;
        }
    }

    /// Replace the queue's context (contracts/transport-and-queue.md §1),
    /// e.g. Settings › Developer's "Play from account" (T063): seeds a
    /// current item so a following `play()` has something to load, and
    /// syncs the program to the source (T067).
    pub fn queue_replace(&mut self, tracks: Vec<modplayer_audio_source::TrackRef>) {
        self.queue.replace_context(tracks);
        self.sync_program();
    }

    /// Move an existing item to the tail of the play-next block (FR-011,
    /// FIFO; contracts/transport-and-queue.md §1).
    pub fn queue_play_next(&mut self, uid: QueueItemId) {
        self.queue.ensure_host_driven();
        self.queue.play_next(uid);
        self.sync_program();
    }

    /// Append a brand-new track directly to the tail of the play-next
    /// block.
    pub fn queue_play_next_track(&mut self, track: modplayer_audio_source::TrackRef) {
        self.queue.ensure_host_driven();
        self.queue.play_next_track(track);
        self.sync_program();
    }

    /// Move `uid` one slot earlier in the effective order (FR-012).
    pub fn queue_move_up(&mut self, uid: QueueItemId) {
        self.queue.ensure_host_driven();
        self.queue.move_up(uid);
        self.sync_program();
    }

    /// Move `uid` one slot later in the effective order (FR-012).
    pub fn queue_move_down(&mut self, uid: QueueItemId) {
        self.queue.ensure_host_driven();
        self.queue.move_down(uid);
        self.sync_program();
    }

    /// Move `uid` to `to_effective_index` (0 = current; the current item
    /// itself never moves) — the drag-reorder target index (FR-012).
    pub fn queue_reorder(&mut self, uid: QueueItemId, to_effective_index: usize) {
        self.queue.ensure_host_driven();
        self.queue.reorder(uid, to_effective_index);
        self.sync_program();
    }

    /// Remove `uid` from wherever it is (FR-012). Removing the current
    /// item skips forward past it.
    pub fn queue_remove(&mut self, uid: QueueItemId) {
        self.queue.ensure_host_driven();
        self.queue.remove(uid);
        self.sync_program();
    }

    /// Turn shuffle on/off (FR-013).
    pub fn set_shuffle(&mut self, on: bool) {
        self.queue.ensure_host_driven();
        self.queue.set_shuffle(on, &mut self.queue_rng);
        self.sync_program();
    }

    /// Set the repeat mode (FR-013/014).
    pub fn set_repeat(&mut self, mode: Repeat) {
        self.queue.ensure_host_driven();
        self.queue.set_repeat(mode);
        self.sync_program();
    }

    /// The Queue panel's projection of the queue (T066, contracts/
    /// transport-and-queue.md §1): the effective order, current item
    /// first, history excluded.
    pub fn queue_view(&self) -> QueueView {
        let current_uid = self.queue.current().map(|item| item.uid);
        let items = self
            .queue
            .effective_order()
            .into_iter()
            .map(|item| QueueRow {
                uid: item.uid,
                title: item.track.title.clone(),
                artist: item.track.artists.join(", "),
                origin: item.origin,
                unavailable: item.unavailable,
                is_current: Some(item.uid) == current_uid,
            })
            .collect();
        QueueView {
            items,
            shuffle: self.queue.shuffle_enabled(),
            repeat: self.queue.repeat(),
        }
    }

    /// Recompute the `Program` from the queue's current effective order
    /// and, when it differs from the last one actually sent (order,
    /// cursor, repeat flags — contracts/transport-and-queue.md §3), queue
    /// it for `flush_pending_program` — which sends immediately if the
    /// debounce window has elapsed, or on a later `tick()` otherwise. A
    /// no-op while the queue is `SourceDriven` or empty. Near the wrap of
    /// a shuffled `repeat == All` cycle, sends the wrap preview
    /// (`Queue::wrap_preview`) instead of the (otherwise single-item)
    /// effective order, so the source can preload the next cycle's first
    /// item gaplessly.
    fn sync_program(&mut self) {
        if self.queue.mode() != QueueMode::HostDriven {
            return;
        }
        let Some(queue_program) = self.queue.program() else {
            self.pending_program = None;
            return;
        };
        let (order, cursor_index) = if queue_program.order.len() <= 1 {
            match self.queue.wrap_preview(&mut self.queue_rng) {
                Some(order) => (order, 0),
                None => (queue_program.order, queue_program.cursor_index),
            }
        } else {
            (queue_program.order, queue_program.cursor_index)
        };
        let position_ms = u32::try_from(self.position().as_millis()).unwrap_or(u32::MAX);
        let start_playing = self.transport_state.intent == Intent::Playing;
        let repeat_all = self.queue.repeat() == Repeat::All;
        let repeat_one = self.queue.repeat() == Repeat::One;

        let unchanged = self.last_sent_program_key.as_ref().is_some_and(
            |(sent_order, sent_cursor, sent_all, sent_one)| {
                *sent_order == order
                    && *sent_cursor == cursor_index
                    && *sent_all == repeat_all
                    && *sent_one == repeat_one
            },
        );
        if unchanged {
            self.pending_program = None;
            return;
        }

        self.pending_program = Some(PendingProgram {
            order,
            cursor_index,
            position_ms,
            start_playing,
            repeat_all,
            repeat_one,
        });
        self.flush_pending_program();
    }

    /// Send `pending_program`, if any, provided the debounce window has
    /// elapsed since the last actual send; otherwise leaves it queued for
    /// a later call (the next `sync_program` or `tick()`).
    fn flush_pending_program(&mut self) {
        let Some(pending) = self.pending_program.clone() else {
            return;
        };
        let now = (self.now)();
        if let Some(last_sent) = self.last_program_sent_at
            && now.saturating_duration_since(last_sent) < PROGRAM_SEND_DEBOUNCE
        {
            return;
        }
        self.pending_program = None;
        self.program_generation += 1;
        self.transport_state.current_generation = self.program_generation;
        let program = Program {
            order: pending.order.clone(),
            cursor_index: pending.cursor_index,
            position_ms: pending.position_ms,
            start_playing: pending.start_playing,
            repeat_all: pending.repeat_all,
            repeat_one: pending.repeat_one,
            generation: self.program_generation,
        };
        self.last_sent_program_key = Some((
            pending.order,
            pending.cursor_index,
            pending.repeat_all,
            pending.repeat_one,
        ));
        self.last_program_sent_at = Some(now);
        self.source_host
            .command(SourceCommand::LoadProgram(program));
    }

    /// Retry any commands that failed to enqueue on a previous attempt
    /// because the SPSC queue was momentarily full (contracts/engine-
    /// commands.md rule 3: the controller "never blocks and never drops
    /// the shadow update"), drain any device events the backend has
    /// raised (device loss, list changes, sample-rate changes — US3
    /// T069-T074), and drain+reduce every event the source host has
    /// raised (contracts/transport-and-queue.md §1's `tick()`; only the
    /// T11/T12 mirror is wired this phase — health/session/transfer
    /// mirroring lands with US3/US4). Call once per UI tick.
    pub fn tick(&mut self) {
        self.flush_pending_commands();
        self.drain_backend_events();
        self.drain_source_events();
        // A `sync_program` send debounced on a previous call becomes
        // sendable once enough real time has passed, even with no further
        // mutation (contracts/transport-and-queue.md §3).
        self.flush_pending_program();
        self.flush_transfer_timer();
        self.flush_reconnect_timer();
    }

    /// T19: fire `Input::TransferTimedOut` once the 5 s transfer-request
    /// deadline (T17) has passed, polled each `tick()` against the
    /// injectable clock (design note 6) rather than a background thread —
    /// self-healing if the state already moved on (`BecameActive`) without
    /// an explicit `Timer(Cancel)` in between.
    fn flush_transfer_timer(&mut self) {
        let Some(deadline) = self.transfer_timer_deadline else {
            return;
        };
        if !matches!(
            self.transport_state.active,
            ActiveState::TransferRequested { .. }
        ) {
            self.transfer_timer_deadline = None;
            return;
        }
        if (self.now)() >= deadline {
            self.transfer_timer_deadline = None;
            self.dispatch(Input::TransferTimedOut);
        }
    }

    /// T20: fire `Input::ReconnectTimedOut` once the 30 s reconnect-
    /// warning deadline has passed, polled each `tick()` against the
    /// injectable clock exactly like `flush_transfer_timer` — self-healing
    /// if health already recovered (`Health(Ok)`) without an explicit
    /// `Timer(Cancel)` racing in first.
    fn flush_reconnect_timer(&mut self) {
        let Some(deadline) = self.reconnect_timer_deadline else {
            return;
        };
        if !matches!(self.transport_state.health, SourceHealth::Transient { .. }) {
            self.reconnect_timer_deadline = None;
            return;
        }
        if (self.now)() >= deadline {
            self.reconnect_timer_deadline = None;
            self.dispatch(Input::ReconnectTimedOut);
        }
    }

    /// Poll `source_host` and mirror every event this phase's reducer
    /// understands through `transport::reduce` (T11/T12).
    fn drain_source_events(&mut self) {
        let events = self.source_host.poll();
        for event in events {
            if let SourceEvent::AccountTracks { request_id, result } = event {
                // Not a transport input — a "Play from account" read reply
                // (Stage 2); stash for the UI to drain via
                // `take_account_tracks`.
                self.pending_account_tracks.push((request_id, result));
                continue;
            }
            if let Some(input) = map_source_event(event) {
                self.dispatch(input);
            }
        }
    }

    /// Ask the source to list up to `limit` playable tracks from the
    /// signed-in account via its own session (Stage 2 "Play from account").
    /// Returns the request id whose reply arrives through
    /// [`Self::take_account_tracks`].
    pub fn request_account_tracks(&mut self, limit: u8) -> u64 {
        let request_id = self.next_account_read_id;
        self.next_account_read_id += 1;
        self.source_host
            .command(SourceCommand::ListAccountTracks { request_id, limit });
        request_id
    }

    /// Drain the `request_account_tracks` replies received since the last
    /// call (one per completed request, in arrival order).
    pub fn take_account_tracks(&mut self) -> Vec<(u64, Result<Vec<TrackRef>, AccountReadError>)> {
        std::mem::take(&mut self.pending_account_tracks)
    }

    /// Run `input` through the pure transport reducer and apply the
    /// effects it returns (contracts/transport-and-queue.md §2). The
    /// `QueueChangeOrigin` a follow-up `Input::QueueChanged` should carry
    /// is derived from which kind of input this was, *before* it is moved
    /// into `reduce`.
    fn dispatch(&mut self, input: Input) {
        let queue_origin = match &input {
            Input::SkipForward => QueueChangeOrigin::UserSkipForward,
            Input::SkipBack { .. } => QueueChangeOrigin::UserSkipBack,
            Input::Seek { .. } => QueueChangeOrigin::UserSeek,
            Input::Unavailable { .. } => QueueChangeOrigin::UnavailableMirror,
            _ => QueueChangeOrigin::EndOfTrackMirror,
        };
        let (new_state, effects) =
            transport::reduce(std::mem::take(&mut self.transport_state), input);
        self.transport_state = new_state;
        self.apply_effects(effects, queue_origin);
    }

    fn apply_effects(&mut self, effects: Vec<Effect>, queue_origin: QueueChangeOrigin) {
        for effect in effects {
            match effect {
                Effect::Engine(cmd) => self.push_command_retrying(cmd),
                Effect::Source(cmd) => {
                    // T22/T23 send `Deregister` straight from the reducer
                    // (unlike `clear_for_sign_out`/`set_playback_
                    // permitted`, which already track this themselves) —
                    // keep `registered_or_pending` in sync so a later
                    // `set_playback_permitted(true, ..)` (tier restored)
                    // sends a fresh `Initialize` rather than assuming the
                    // source is still registered.
                    if matches!(cmd, SourceCommand::Deregister) {
                        self.registered_or_pending = false;
                    }
                    self.source_host.command(cmd);
                }
                Effect::SeekTo { position_ms } => {
                    let frames = self.ms_to_frames(position_ms);
                    self.push_command_retrying(Command::Seek(frames));
                    self.source_host.command(SourceCommand::Seek(position_ms));
                }
                Effect::LoadCurrentProgram {
                    position_ms,
                    start_playing,
                } => {
                    if let Some(program) = self.build_program(position_ms, start_playing) {
                        self.source_host
                            .command(SourceCommand::LoadProgram(program));
                    }
                }
                Effect::Queue(op) => {
                    let change = self.apply_queue_op(op);
                    self.dispatch(Input::QueueChanged {
                        change,
                        origin: queue_origin,
                    });
                }
                Effect::Notify {
                    key,
                    severity,
                    actions,
                } => {
                    if actions.is_empty() {
                        self.notifications.raise(severity, key);
                    } else {
                        self.notifications
                            .raise_with_actions(severity, key, Vec::new(), actions);
                    }
                }
                Effect::DismissNotify { key } => {
                    self.notifications.dismiss_by_key(key);
                }
                Effect::Timer(TimerCommand::Start(TimerKind::Transfer)) => {
                    self.transfer_timer_deadline = Some((self.now)() + TRANSFER_TIMEOUT);
                }
                Effect::Timer(TimerCommand::Cancel(TimerKind::Transfer)) => {
                    self.transfer_timer_deadline = None;
                }
                Effect::Timer(TimerCommand::Start(TimerKind::Reconnect)) => {
                    self.reconnect_timer_deadline = Some((self.now)() + RECONNECT_WARNING_TIMEOUT);
                }
                Effect::Timer(TimerCommand::Cancel(TimerKind::Reconnect)) => {
                    self.reconnect_timer_deadline = None;
                }
                Effect::ApplyPendingTransferCommand(pending) => match pending {
                    PendingTransferCommand::Play | PendingTransferCommand::PlayHere => self.play(),
                    PendingTransferCommand::SkipForward => self.skip_forward(),
                    PendingTransferCommand::SkipBack => self.skip_back(),
                    PendingTransferCommand::Seek(ms) => {
                        self.seek(Duration::from_millis(u64::from(ms)));
                    }
                },
                Effect::MirrorVolume(pct) => {
                    // T15: update the shadow/engine volume like
                    // `set_master_volume`, but do not echo
                    // `SourceCommand::SetVolume` back to the source that
                    // just reported this remote change.
                    let volume = VolumePercent::new(pct.value());
                    self.master_volume = volume;
                    self.push_command_retrying(Command::SetMasterVolume(volume));
                    self.persist_settings(|settings| settings.master_volume = volume);
                }
                Effect::RemoteSetShuffle(on) => self.set_shuffle(on),
                Effect::RemoteSetRepeat(mode) => self.set_repeat(mode),
            }
        }
    }

    /// Apply `op` to the queue, then keep applying `advance(Removed)`
    /// while the result is `Skipped(uid)` — `Queue::advance`/`mark_
    /// unavailable` return `Skipped` one item at a time, expecting the
    /// caller to keep walking forward past every already-unavailable item
    /// until a terminal result (FR-026), notifying about each one along
    /// the way (contracts/transport-and-queue.md §2 rule T14).
    fn apply_queue_op(&mut self, op: QueueOp) -> QueueChange {
        let mut change = match op {
            QueueOp::Advance(reason) => self.queue.advance(reason, &mut self.queue_rng),
            QueueOp::SkipBack { position_ms } => self.queue.skip_back(position_ms),
            QueueOp::Reveal(track) => {
                let uid = self.queue.reveal(track);
                self.queue.move_current_to(uid)
            }
            QueueOp::MarkUnavailable(track_id) => {
                self.notify_track_unavailable(&track_id);
                self.queue.mark_unavailable(&track_id, &mut self.queue_rng)
            }
            QueueOp::AdoptTransferContext {
                track,
                shuffle,
                repeat,
            } => {
                let change = self.queue.adopt_transfer_context(track);
                if let Some(on) = shuffle {
                    self.queue.set_shuffle(on, &mut self.queue_rng);
                }
                if let Some(mode) = repeat {
                    self.queue.set_repeat(mode);
                }
                change
            }
        };
        while let PlaybackChange::Skipped(uid) = change.playback {
            self.notify_queue_item_skipped(uid);
            change = self
                .queue
                .advance(AdvanceReason::Removed, &mut self.queue_rng);
        }
        // The host already knows every queued item's duration (unlike a
        // source-revealed track, which needs the `TrackStarted` mirror,
        // T12) — keep the reducer's `track_len_ms` in sync with whichever
        // item `Queue` is now on, so seek-past-end (T7, SC-006) has a
        // length to compare against right after a skip/advance/reveal
        // rather than only once a `TrackStarted` event round-trips back
        // from the source. `None` when the queue emptied out.
        self.transport_state.track_len_ms = self.queue.current().map(|item| item.track.duration_ms);
        change
    }

    /// Raise `queue-item-skipped-unavailable` for the track the source
    /// just refused (FR-026), looked up by id before `mark_unavailable`
    /// runs — the reducer only has the id, not the title.
    fn notify_track_unavailable(&mut self, track_id: &TrackId) {
        let title = self.queue.track_title(track_id).unwrap_or_default();
        self.notifications.raise_with_args(
            Severity::Info,
            KEY_QUEUE_ITEM_SKIPPED_UNAVAILABLE,
            vec![("title", title)],
        );
    }

    /// Raise `queue-item-skipped-unavailable` for an item `advance`
    /// stepped over because it was already marked unavailable (FR-026),
    /// looked up by uid before it potentially falls out of the queue.
    fn notify_queue_item_skipped(&mut self, uid: QueueItemId) {
        let title = self
            .queue
            .item(uid)
            .map(|item| item.track.title.clone())
            .unwrap_or_default();
        self.notifications.raise_with_args(
            Severity::Info,
            KEY_QUEUE_ITEM_SKIPPED_UNAVAILABLE,
            vec![("title", title)],
        );
    }

    /// Build a `Program` from the queue's current effective order
    /// (contracts/transport-and-queue.md §3), bumping the program
    /// generation and keeping `transport_state.current_generation` in
    /// sync so T12's staleness check is meaningful. `None` when the queue
    /// has nothing to play.
    fn build_program(
        &mut self,
        position_ms: u32,
        start_playing: bool,
    ) -> Option<modplayer_audio_source::Program> {
        let queue_program = self.queue.program()?;
        self.program_generation += 1;
        self.transport_state.current_generation = self.program_generation;
        Some(queue_program.into_program(
            position_ms,
            start_playing,
            self.queue.repeat() == Repeat::All,
            self.queue.repeat() == Repeat::One,
            self.program_generation,
        ))
    }

    /// Convert a millisecond position to source-rate frames, using the
    /// sample rate captured at the last `attach()` (0 before any stream
    /// has ever been opened, so this yields frame 0).
    fn ms_to_frames(&self, position_ms: u32) -> u64 {
        (u64::from(position_ms) * u64::from(self.source_sample_rate)) / 1000
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
                self.transport_state.intent = Intent::Paused;
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
        // A fresh `Rt` per stream (re)build, restored to the last known
        // position; never cached across streams (contracts/audio-source-
        // host.md §1, design note 2).
        let source = self.source_host.attach(self.shared.position_frames());
        self.source_sample_rate = source.sample_rate();
        let config = ProcessorConfig {
            source_rate: source.sample_rate(),
            device_rate: device.default_rate.hz(),
            device_channels: device.channels,
            max_frames: self.preset.requested_frames().frames() as usize,
            transport: self.engine_transport(),
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

/// Map a `SourceEvent` to the `Input` this phase's reducer understands
/// (T1-T14's minimal subset, plus a lightweight `Registered`/
/// `Deregistered`/`TierRejected`/`Health` mirror so `active_state()`/
/// `disabled_reason()` are meaningful from US1 on; `None` for every event
/// whose full rule lands with a later user story — contracts/audio-
/// source-host.md §3, contracts/transport-and-queue.md §2).
fn map_source_event(event: SourceEvent) -> Option<Input> {
    match event {
        SourceEvent::TrackStarted {
            track,
            program,
            position_ms,
            playing,
        } => match program {
            Some((generation, _index)) => Some(Input::TrackStarted {
                generation,
                position_ms,
                playing,
                track_len_ms: track.duration_ms,
            }),
            None => Some(Input::TrackRevealed { track }),
        },
        SourceEvent::EndOfTrack => Some(Input::EndOfTrack),
        SourceEvent::Registered { .. } => Some(Input::Registered),
        SourceEvent::Deregistered => Some(Input::Deregistered),
        SourceEvent::TierRejected => Some(Input::TierRejected),
        SourceEvent::Health(health) => Some(Input::Health(health)),
        SourceEvent::Loading { .. } => Some(Input::Loading),
        SourceEvent::Playing { .. } => Some(Input::Playing),
        SourceEvent::Unavailable { track } => Some(Input::Unavailable { track }),
        SourceEvent::BecameActive { context } => Some(Input::BecameActive { context }),
        SourceEvent::BecameInactive => Some(Input::BecameInactive),
        SourceEvent::RemoteCommand(cmd) => Some(Input::RemoteCommand(cmd)),
        // Paused/Stopped/Seeked's full rules (T20/T21) are US4's job.
        _ => None,
    }
}

/// `"ModPlayer on <hostname>"`, falling back to plain `"ModPlayer"` when
/// the hostname cannot be determined (FR-001's default name). Shells out
/// to the platform's own `hostname` command (present on macOS, Linux and
/// Windows alike) rather than adding a dependency for a single syscall.
fn default_device_name() -> String {
    let hostname = std::process::Command::new("hostname")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    match hostname {
        Some(host) => format!("ModPlayer on {host}"),
        None => "ModPlayer".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use modplayer_audio_io::FakeBackend;
    use modplayer_audio_source_synthetic::SyntheticHost;

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
        let controller = PlaybackController::new(
            FakeBackend::new(vec![]),
            SyntheticHost::new(44_100),
            fresh_store(),
        );
        assert_eq!(controller.transport(), Transport::Stopped);
    }

    #[test]
    fn shadow_state_seeded_from_defaults_with_no_settings_file() {
        let controller = PlaybackController::new(
            FakeBackend::new(vec![]),
            SyntheticHost::new(44_100),
            fresh_store(),
        );
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
        let controller = PlaybackController::new(
            FakeBackend::new(vec![]),
            SyntheticHost::new(44_100),
            fresh_store(),
        );
        let shared = controller.shared();
        shared.advance_clock(42);
        assert_eq!(controller.shared().clock_frames(), 42);
    }

    #[test]
    fn a_settings_load_warning_becomes_a_notification() {
        let store = fresh_store();
        let _ = std::fs::write(store.path(), "not valid toml {{{");
        let controller =
            PlaybackController::new(FakeBackend::new(vec![]), SyntheticHost::new(44_100), store);
        assert_eq!(controller.notifications().visible().count(), 1);
        let notification = controller.notifications().visible().next();
        assert_eq!(notification.map(|n| n.severity), Some(Severity::Warning));
        assert_eq!(
            notification.map(|n| n.message_key),
            Some("settings-unreadable")
        );
    }
}

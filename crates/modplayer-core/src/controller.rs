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

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant};

use modplayer_audio_io::{BackendEvent, OpenStream, OutputBackend, OutputDeviceInfo};
use modplayer_audio_source::{
    AudioSource, CatalogError, LibraryPage, Program, Repeat, SourceCommand, SourceEvent,
    SourceHealth, SourceHost, TrackId, TrackList, TrackListSource, TrackRef,
};
use modplayer_engine::{
    BufferPreset, CeilingDb, Command, DeviceId, Event, NegotiatedBuffer, PositionClock, Processor,
    ProcessorConfig, RtShared, SampleRate, Theme, Transport, VolumePercent,
};
use rtrb::{Consumer, Producer, RingBuffer};

use crate::actions::{ActionRegistry, BindingError, Chord, HostAction};
use crate::analysis::{AnalysisPaths, AnalysisService, AnalysisSnapshot};
use crate::device_policy::{self, DeviceLostOutcome, DeviceResolution, DeviceWarning};
use crate::library::index::SyncOutcome;
use crate::library::{
    Connectivity, LibraryIndex, LibraryPaths, LibraryStatus, LoadIndexOutcome, LoadPlayLogOutcome,
    PersistJob, PlayLog, SyncScheduler,
};
use crate::markers::{
    self, CueSlot, MarkerError, MarkerId, MarkerKind, PaletteIndex, RegionId, RepeatCount,
    TrackMarkers,
};
use crate::notifications::{
    KEY_DEVICE_APPEARED, KEY_DEVICE_AVAILABLE_AGAIN, KEY_DEVICE_LOST, KEY_DEVICE_MISSING_AT_LAUNCH,
    KEY_NO_OUTPUT_DEVICES, KEY_QUEUE_ITEM_SKIPPED_UNAVAILABLE, KEY_TRACK_STATE_NEWER_VERSION,
    KEY_TRACK_STATE_SAVE_FAILED, KEY_TRACK_STATE_UNREADABLE, NotificationCenter, Severity,
};
use crate::queue::{
    AdvanceReason, Origin, PlaybackChange, Queue, QueueChange, QueueItem, QueueItemId, QueueMode,
    XorShiftRng,
};
use crate::search::SearchSession;
use crate::settings::{
    AudioSettings, DeviceName, DeviceNameError, SettingsStore, SettingsWarning,
    generate_connect_device_id,
};
use crate::transport::{
    self, ActiveState, Effect, Input, Intent, NotRegisteredReason, PendingTransferCommand,
    QueueChangeOrigin, QueueOp, TimerCommand, TimerKind, TransportState,
};

/// contracts/library-and-search-core.md §4: at most this many ids per
/// `HydrateRefs` sweep request.
const HYDRATE_BATCH: usize = 100;
/// contracts/library-and-search-core.md §4: "≤ 2 batches/s background
/// sweep" — one sweep request at most every 500 ms.
const HYDRATE_SWEEP_INTERVAL: Duration = Duration::from_millis(500);
/// contracts/library-and-search-core.md §4: dirty persistence flushes at
/// most once per this window.
const PERSIST_DEBOUNCE: Duration = Duration::from_secs(1);

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

/// 006, research R5: the coalesced `SourceCommand::Seek` that follows a
/// loop wrap is throttled to at most one per this interval after the
/// first (sent immediately).
const LOOP_RESEEK_INTERVAL: Duration = Duration::from_millis(250);

/// 006, research R11: a dirty marker/loop-region state is written at most
/// once per this window (checked in `tick()`); `flush_track_state()`
/// forces it outside the window (track change, sign-out, shutdown,
/// `clear_all_markers`).
const TRACK_STATE_DEBOUNCE: Duration = Duration::from_millis(250);

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

/// `RtShared::loop_state`, projected for the UI (006, contracts/marker-
/// service.md §1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopState {
    Disarmed,
    ArmedInactive,
    ArmedActive,
}

/// `PlaybackController::loop_status()`'s snapshot of the armed region's
/// real-time state (006, contracts/marker-service.md §1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoopStatus {
    pub state: LoopState,
    pub wraps: u32,
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
    /// `[markers] nudge_step_ms` shadow state (006, FR-027): how far a
    /// focused marker's arrow-key nudge moves it, in milliseconds; clamped
    /// `1..=1000`.
    nudge_step_ms: u16,
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
    /// Free-text catalog search state (004-search-and-library-browse,
    /// research R7). `tick()` drives its debounce and routes
    /// `SourceEvent::SearchResult` replies into it by `request_id`.
    search: SearchSession,

    /// The account library mirror (004-search-and-library-browse,
    /// data-model.md §3.1). `Default`/empty until the background load
    /// (`library_loading`) completes.
    library: LibraryIndex,
    /// Recently Played (data-model.md §3.5, research R11).
    play_log: PlayLog,
    /// The `FetchLibrary` cycle scheduler (data-model.md §3.3).
    sync: SyncScheduler,
    /// `None` when `library::LibraryPaths::resolve()` couldn't determine a
    /// data directory (no platform home dir) — persistence degrades to
    /// in-memory-only rather than failing launch.
    persist_tx: Option<Sender<PersistJob>>,
    /// The persistence writer thread, joined on `shutdown` so its last
    /// queued write completes before the process exits.
    persist_writer: Option<std::thread::JoinHandle<()>>,
    /// The background-load reply channel, drained by `tick()`; `None`
    /// once drained (or if no paths were resolvable).
    load_rx: Option<Receiver<(LoadIndexOutcome, LoadPlayLogOutcome)>>,
    /// `true` until the background load completes (contracts/ui-surface.md
    /// §3 "loading" state) — starts `false` when no paths were resolvable
    /// (nothing to wait for).
    library_loading: bool,
    /// Set by any `library`/`play_log` mutation since the last flush;
    /// cleared by `flush_library_persistence`.
    library_dirty: bool,
    /// `MODPLAYER_LIBRARY_FIXTURE=large` has replaced `library` for this
    /// process (debug builds only; see `tick_library`).
    #[cfg(debug_assertions)]
    large_fixture_seeded: bool,
    last_persist_flush_at: Option<Instant>,
    last_hydration_sweep_at: Option<Instant>,
    /// Monotonic id source for `FetchTrackList`/`HydrateRefs` commands —
    /// separate from `search`'s own generation-packed ids (a different
    /// `SourceEvent` variant, so no collision risk either way).
    next_library_request_id: u64,
    /// `TrackListSource -> request_id` for an outstanding
    /// `FetchTrackList` (contracts/library-and-search-core.md §1
    /// `library_track_list`), so a second call while one is in flight
    /// doesn't issue a duplicate request.
    track_list_in_flight: HashMap<TrackListSource, u64>,
    /// The track id last recorded into `play_log`, so pausing/resuming the
    /// same track never double-records it (research R11: record only on a
    /// `TrackStarted -> Playing` transition to a *different* track than
    /// the last one recorded).
    last_recorded_play_track: Option<TrackId>,
    /// Edge-detects the offline -> online transition so `tick_library` can
    /// fire an immediate `Reconnect` trigger (FR-011) rather than waiting
    /// for the passive 15-min `Interval` check — also doubles as the
    /// `Launch` trigger, since the very first `tick()` that finds the
    /// controller online is exactly that edge.
    was_online_for_sync: bool,

    /// The Analysis Service (005-now-playing-waveform, contracts/
    /// analysis-service.md, contracts/transport-delta.md §2): folds the
    /// current track's `DecodedStore` into waveform peaks off the UI
    /// thread. `tick()` drains it; `analysis()` exposes the latest
    /// snapshot.
    analysis: AnalysisService,
    /// The track id `analysis` is currently attached to (or was last
    /// synced against), so `sync_analysis_attachment` only detaches/
    /// reattaches on an actual change of `queue.current()` (transport-
    /// delta.md §2 "current track changes (any path)").
    last_analysis_track: Option<TrackId>,

    /// The current track's marker/loop-region shadow state (006,
    /// contracts/marker-service.md §1), `None` until first touched.
    /// Recreated (in-memory only) whenever it doesn't match the current
    /// track's id — this phase (US1) has no persistence yet; US2's
    /// `sync_marker_attachment` replaces this lazy-create with a real
    /// load/save cycle.
    markers: Option<TrackMarkers>,
    /// Last time the controller sent a coalesced `SourceCommand::Seek`
    /// to follow a loop wrap (research R5); `None` until the first one.
    last_loop_reseek_at: Option<Instant>,

    /// Resolved track-state directory (006, contracts/marker-service.md
    /// §3, research R10), `None` when the platform data-local dir isn't
    /// determinable (in-memory markers only, like `AnalysisPaths`).
    track_state_paths: Option<markers::store::TrackStatePaths>,
    /// The background writer's job channel, `None` when
    /// `track_state_paths` is `None`.
    marker_persist_tx: Option<Sender<markers::store::PersistJob>>,
    /// Joined on `shutdown`, after the writer's queued saves complete.
    marker_persist_writer: Option<std::thread::JoinHandle<()>>,
    /// The background writer's failure replies, drained by `tick()`.
    marker_store_rx: Option<Receiver<markers::store::StoreEvent>>,
    /// The track id `markers` was last synced against (contracts/
    /// marker-service.md §4) — unlike `last_analysis_track`, also
    /// re-triggers on a same-id `TrackStarted` (FR-016 "same-session
    /// reload").
    marker_state_track: Option<TrackId>,
    /// Whether the next `Input::TrackStarted` is the *initial* start of the
    /// attachment `sync_marker_attachment` just made (set when it loaded
    /// because the id changed, with no `TrackStarted` of its own). That one
    /// start must not re-load the state a second time — without this the
    /// single act of starting a track loads twice and raises any load
    /// warning (`track-state-unreadable`) twice. A `TrackStarted` arriving
    /// with this clear is a genuine same-id restart (FR-016) and does
    /// reload.
    marker_state_awaiting_start: bool,
    /// Last time the debounced marker-state flush actually wrote
    /// (research R11, `TRACK_STATE_DEBOUNCE`); `None` until the first one.
    last_marker_flush_at: Option<Instant>,

    /// The Action & Binding registry (007, data-model.md §3.1): shadow
    /// state seeded from `settings.keybinding_overrides` at construction
    /// and persisted, through this controller's existing
    /// `persist_settings`, on every binding mutation.
    actions: ActionRegistry,
}

/// `transport.seek_forward_step`/`seek_backward_step`'s step size
/// (FR-004a; research R9) — the same 5 s 003's seek slider and 005's
/// waveform keys already use.
pub const SEEK_STEP: Duration = Duration::from_secs(5);

/// `transport.volume_up`/`volume_down`'s step size (FR-004a; research
/// R9) — 5 % of the 0-100 % scale.
pub const VOLUME_STEP: u8 = 5;

/// [`PlaybackController::library_track_list`]'s reply state (contracts/
/// library-and-search-core.md §1).
#[derive(Debug, Clone, PartialEq)]
pub enum TrackListState {
    Cached(Vec<TrackRef>),
    Loading,
    Failed,
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
        for warning in &outcome.warnings {
            raise_settings_warning(&mut notifications, warning);
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
        let mut controller = Self {
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
            nudge_step_ms: settings.nudge_step_ms,
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
            search: SearchSession::new(),
            library: LibraryIndex::new(),
            play_log: PlayLog::new(),
            sync: SyncScheduler::new(),
            // No persistence until `with_library_paths` opts in
            // explicitly (the binary's job, `main.rs`) — a constructor
            // that reached out to the real platform data directory by
            // default would make every one of this controller's many
            // existing unit tests spawn real background disk I/O against
            // the developer's actual `ModPlayer` data directory.
            persist_tx: None,
            persist_writer: None,
            load_rx: None,
            library_loading: false,
            library_dirty: false,
            #[cfg(debug_assertions)]
            large_fixture_seeded: false,
            last_persist_flush_at: None,
            last_hydration_sweep_at: None,
            next_library_request_id: 0,
            track_list_in_flight: HashMap::new(),
            last_recorded_play_track: None,
            was_online_for_sync: false,
            analysis: AnalysisService::new(AnalysisPaths::resolve()),
            last_analysis_track: None,
            markers: None,
            last_loop_reseek_at: None,
            track_state_paths: None,
            marker_persist_tx: None,
            marker_persist_writer: None,
            marker_store_rx: None,
            marker_state_track: None,
            marker_state_awaiting_start: false,
            last_marker_flush_at: None,
            actions: ActionRegistry::new(settings.keybinding_overrides.clone()),
        };
        // 006, contracts/marker-service.md §4: resolved unconditionally at
        // construction, like `AnalysisPaths::resolve()` just above —
        // `None` (no determinable data dir) degrades to in-memory-only
        // markers, never a launch failure.
        if let Some(paths) = markers::store::TrackStatePaths::resolve() {
            let (store_tx, store_rx) = std::sync::mpsc::channel();
            let (persist_tx, writer) = markers::store::spawn_writer(store_tx);
            controller.track_state_paths = Some(paths);
            controller.marker_persist_tx = Some(persist_tx);
            controller.marker_persist_writer = Some(writer);
            controller.marker_store_rx = Some(store_rx);
        }
        controller
    }

    /// Current transport-reducer shadow state (data-model.md §3.1).
    pub fn transport_state(&self) -> &TransportState {
        &self.transport_state
    }

    /// The host-owned play queue (data-model.md §2.2).
    pub fn queue(&self) -> &Queue {
        &self.queue
    }

    /// Free-text catalog search state (004-search-and-library-browse,
    /// contracts/library-and-search-core.md §1).
    pub fn search(&self) -> &SearchSession {
        &self.search
    }

    /// Edit-access to search state: `search_mut().set_query(text, now)` on
    /// every keystroke (contracts/library-and-search-core.md §1).
    pub fn search_mut(&mut self) -> &mut SearchSession {
        &mut self.search
    }

    /// **Show more** on one search group (contracts/library-and-search-
    /// core.md §1, FR-002): a no-op when that group has no further page or
    /// a page request is already outstanding for it.
    pub fn search_show_more(&mut self, kind: modplayer_audio_source::SearchKind) {
        if let Some(cmd) = self.search.show_more(kind) {
            self.source_host.command(cmd);
        }
    }

    /// Opt this controller into real persistence (research R6): spawns the
    /// background loader and the long-lived background writer against
    /// `paths`, and marks the library `loading` until the load completes.
    /// `None` leaves the library in-memory-only (this controller's
    /// default) — the binary is the only caller that passes `Some`
    /// (`LibraryPaths::resolve()`, `main.rs`), so unit tests across every
    /// crate that construct a bare `PlaybackController::new(..)` never
    /// touch a real directory on disk.
    #[must_use]
    pub fn with_library_paths(mut self, paths: Option<LibraryPaths>) -> Self {
        if let Some(paths) = paths {
            let (tx, writer) = crate::library::persist::spawn_writer(paths.clone());
            self.persist_tx = Some(tx);
            self.persist_writer = Some(writer);
            self.load_rx = Some(crate::library::persist::spawn_background_load(paths));
            self.library_loading = true;
        }
        self
    }

    /// The account library mirror snapshot (004-search-and-library-browse,
    /// contracts/library-and-search-core.md §1) — never blocks; may still
    /// be `loading` (`library_status()`).
    pub fn library(&self) -> &LibraryIndex {
        &self.library
    }

    /// The Library view's derived state flags (contracts/library-and-
    /// search-core.md §1).
    pub fn library_status(&self) -> LibraryStatus {
        LibraryStatus {
            loading: self.library_loading,
            refreshing: self.sync.refreshing(),
            first_sync_failed: self.library.meta().first_sync_failed(),
            connectivity: if self.is_online() {
                Connectivity::Online
            } else {
                Connectivity::Offline
            },
        }
    }

    /// FR-021's retry action: force a fresh sync cycle regardless of the
    /// 15-min interval or current backoff.
    pub fn library_retry_sync(&mut self) {
        let now = (self.now)();
        for cmd in self.sync.trigger(now) {
            self.source_host.command(cmd);
        }
    }

    /// The full ordered track list for an album/playlist/artist row
    /// (contracts/library-and-search-core.md §1 "Acting-list rule"):
    /// serves the cache, or issues `FetchTrackList` on first miss (a
    /// second call while that request is outstanding reports `Loading`
    /// rather than issuing a duplicate).
    pub fn library_track_list(&mut self, source: TrackListSource) -> TrackListState {
        if let Some(tracks) = self.library.track_list(&source) {
            return TrackListState::Cached(tracks);
        }
        if self.track_list_in_flight.contains_key(&source) {
            return TrackListState::Loading;
        }
        let request_id = self.next_library_request_id;
        self.next_library_request_id += 1;
        self.track_list_in_flight.insert(source.clone(), request_id);
        self.source_host
            .command(SourceCommand::FetchTrackList { request_id, source });
        TrackListState::Loading
    }

    /// UI hint: prioritise `ids` in the hydration sweep (contracts/
    /// library-and-search-core.md §1 `library_hydrate_visible`, design
    /// note 7).
    pub fn library_hydrate_visible(&mut self, ids: &[TrackId]) {
        self.library.prioritize_hydration(ids);
    }

    /// Recently Played (FR-012), newest first, <= 100 entries.
    pub fn recently_played(&self) -> Vec<TrackRef> {
        self.play_log.recent_tracks()
    }

    fn mark_library_dirty(&mut self) {
        self.library_dirty = true;
    }

    /// "Online" for search gating and the sync scheduler (research R5,
    /// data-model.md §3.6 `Connectivity`): the source health is `Ok` and
    /// the device is registered with the service. Computed inline here,
    /// the same way `library::Connectivity` (US2) will, since that module
    /// does not exist yet in this phase.
    fn is_online(&self) -> bool {
        matches!(self.transport_state.health, SourceHealth::Ok)
            && !matches!(
                self.transport_state.active,
                ActiveState::NotRegistered { .. }
            )
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

    /// The source's sample rate (44 100 Hz for Connect), `0` before the
    /// first `SourceEvent::TrackStarted`/`BecameActive` has told the engine
    /// what it is (005-now-playing-waveform): the waveform's `TimeSpace`
    /// needs it to map pixels to the exact frame indices `seek_frames`
    /// expects (contracts/ui-waveform.md §4 "Track length for the
    /// coordinate space").
    pub fn source_sample_rate(&self) -> u32 {
        self.source_sample_rate
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

    /// `[markers] nudge_step_ms` (006, FR-027, contracts/marker-service.md
    /// §1): how far a focused marker's arrow-key nudge moves it, in
    /// milliseconds (`Shift` multiplies by 10).
    pub fn nudge_step_ms(&self) -> u16 {
        self.nudge_step_ms
    }

    /// Clamp `ms` into `1..=1000` and persist it (data-model.md §5
    /// "clamped silently" — unlike `set_device_name`, there is no rejected
    /// shape here, so this never fails).
    pub fn set_nudge_step_ms(&mut self, ms: u16) {
        let ms = ms.clamp(1, 1_000);
        self.nudge_step_ms = ms;
        self.persist_settings(|settings| settings.nudge_step_ms = ms);
    }

    /// The Action & Binding registry (007, contracts/action-registry.md
    /// §5): which chord fires which action right now.
    pub fn actions(&self) -> &ActionRegistry {
        &self.actions
    }

    /// Add `chord` to `action`'s bindings and persist the result
    /// (contracts/action-registry.md §5). A failed write raises
    /// `settings-save-failed` and leaves the in-memory registry changed
    /// (same semantics as `set_nudge_step_ms`).
    pub fn add_binding(&mut self, action: HostAction, chord: Chord) -> Result<(), BindingError> {
        self.actions.add_binding(action, chord)?;
        self.persist_actions();
        Ok(())
    }

    /// Remove `chord` from `action`'s bindings (a no-op if it wasn't
    /// held) and persist the result.
    pub fn remove_binding(&mut self, action: HostAction, chord: Chord) {
        self.actions.remove_binding(action, chord);
        self.persist_actions();
    }

    /// Restore `action`'s catalog default bindings and persist the
    /// result (FR-011).
    pub fn reset_binding(&mut self, action: HostAction) {
        self.actions.reset(action);
        self.persist_actions();
    }

    /// Restore every action's catalog default bindings and persist the
    /// result (FR-011).
    pub fn reset_all_bindings(&mut self) {
        self.actions.reset_all();
        self.persist_actions();
    }

    /// Enable/disable `action` (FR-012). Never persisted: `enabled`
    /// reflects host-component support, not a user preference.
    pub fn set_action_enabled(&mut self, action: HostAction, enabled: bool) {
        self.actions.set_enabled(action, enabled);
    }

    /// Persist the registry's current overrides through the existing
    /// settings gateway (contracts/action-registry.md §5).
    fn persist_actions(&mut self) {
        let overrides = self.actions.overrides().clone();
        self.persist_settings(|settings| settings.keybinding_overrides = overrides.clone());
    }

    /// `transport.seek_forward_step`/`seek_backward_step` (FR-004a,
    /// research R9): move the playhead by [`SEEK_STEP`] (`direction > 0`
    /// forward, else backward), clamped to `[0, track end]` via the
    /// existing `seek` (so 003's seek semantics — including "seeking
    /// past the end advances", FR-014 — apply unchanged). A no-op while
    /// transport is disabled or no track is current, mirroring the
    /// disabled buttons.
    pub fn seek_step(&mut self, direction: i8) {
        if !self.transport_enabled() {
            return;
        }
        let Some(track_len_ms) = self.current_track().map(|item| item.track.duration_ms) else {
            return;
        };
        let track_len = Duration::from_millis(u64::from(track_len_ms));
        let current = self.position();
        let target = if direction >= 0 {
            current.saturating_add(SEEK_STEP).min(track_len)
        } else {
            current.saturating_sub(SEEK_STEP)
        };
        self.seek(target);
    }

    /// `transport.volume_up`/`volume_down` (FR-004a, research R9):
    /// change master volume by [`VOLUME_STEP`] (`direction > 0` up, else
    /// down), saturating into `0..=100`, through the existing
    /// `set_master_volume` (so the engine's smooth gain ramp, the
    /// persisted value and the Connect volume mirror are all reused). A
    /// no-op while transport is disabled, mirroring the disabled slider.
    pub fn step_master_volume(&mut self, direction: i8) {
        if !self.transport_enabled() {
            return;
        }
        let current = i16::from(self.master_volume().value());
        let step = i16::from(VOLUME_STEP);
        let target = if direction >= 0 {
            current + step
        } else {
            current - step
        };
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        let target = target.clamp(0, 100) as u8;
        self.set_master_volume(VolumePercent::new(target));
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
        // `queue` is reset directly above rather than through `dispatch`,
        // so `sync_analysis_attachment`'s post-`dispatch` hook never runs
        // for this change — detach explicitly (contracts/transport-
        // delta.md §2).
        self.analysis.detach();
        self.last_analysis_track = None;
        // 006, contracts/marker-service.md §4: flush + drop + disarm,
        // same ordering as the analysis detach just above — markers are
        // not account data, so unlike `library`/`play_log` the file
        // itself is kept (data-model.md §4 rule 4).
        self.clear_marker_state_for_sign_out();
        if self.registered_or_pending {
            self.registered_or_pending = false;
            self.source_host.command(SourceCommand::Deregister);
        }
        self.transport_state.active = ActiveState::NotRegistered {
            reason: NotRegisteredReason::SignedOut,
        };
        // 004-search-and-library-browse, design note 8: search, the
        // library index, play log and their persisted files are cleared
        // along with everything else on sign-out.
        self.search = SearchSession::new();
        self.library = LibraryIndex::new();
        self.play_log = PlayLog::new();
        self.sync = SyncScheduler::new();
        self.track_list_in_flight.clear();
        self.last_recorded_play_track = None;
        self.was_online_for_sync = false;
        self.library_dirty = false;
        if let Some(tx) = &self.persist_tx {
            let _ = tx.send(PersistJob::DeleteAll);
        }
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
        // Contracts/transport-delta.md §2: after the source `Shutdown`.
        self.analysis.shutdown();
        // 006, contracts/marker-service.md §4: flush whatever is still
        // inside the debounce window, then join the writer, after
        // `analysis.shutdown()` — mirrors the persistence flush/join just
        // below for `library`/`play_log`.
        self.flush_track_state();
        drop(self.marker_persist_tx.take());
        if let Some(writer) = self.marker_persist_writer.take() {
            let _ = writer.join();
        }
        self.stream = None;
        // Anything still inside the persist debounce window (the track that
        // just started, the page that just merged) is written now, and the
        // writer is drained before the process is allowed to exit — the
        // 2026-09-17 manual walk (quickstart M9) otherwise lost the last
        // play-log entry on quit.
        self.last_persist_flush_at = None;
        self.flush_persistence();
        drop(self.persist_tx.take());
        if let Some(writer) = self.persist_writer.take() {
            let _ = writer.join();
        }
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
            position_frames: None,
            buffer_ready,
        });
    }

    /// `seek_frames` (005-now-playing-waveform, contracts/transport-
    /// delta.md §1): sample-accurate seek to an exact source-rate frame —
    /// a waveform click or keyboard seek. `position_ms` is derived from
    /// `frame` at the current `source_sample_rate` (for
    /// `SourceCommand::Seek` and cluster reporting, R8); the exact frame
    /// itself carries through to the engine's `Command::Seek` unchanged by
    /// that ms round trip.
    pub fn seek_frames(&mut self, frame: u64) {
        let position_ms = if self.source_sample_rate == 0 {
            0
        } else {
            u32::try_from((frame * 1000) / u64::from(self.source_sample_rate)).unwrap_or(u32::MAX)
        };
        let buffer_ready = self.source_host.buffer_status().ready;
        self.dispatch(Input::Seek {
            position_ms,
            position_frames: Some(frame),
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

    /// **Play now** on a row picked from a list that is not itself the
    /// current context (004-search-and-library-browse, contracts/library-
    /// and-search-core.md §2, FR-006): the acting list becomes the new
    /// context, playback starts at `cursor`.
    pub fn queue_replace_at(
        &mut self,
        tracks: Vec<modplayer_audio_source::TrackRef>,
        cursor: usize,
    ) {
        self.queue.replace_context_at(tracks, cursor);
        self.sync_program();
    }

    /// **Play next** on an album/playlist/artist row (004-search-and-
    /// library-browse, contracts/library-and-search-core.md §2, FR-007):
    /// appends every track to the tail of the play-next block, preserving
    /// order.
    pub fn queue_play_next_tracks(&mut self, tracks: Vec<modplayer_audio_source::TrackRef>) {
        self.queue.ensure_host_driven();
        self.queue.play_next_tracks(tracks);
        self.sync_program();
    }

    /// **Add to queue** on any row (004-search-and-library-browse,
    /// contracts/library-and-search-core.md §1, FR-007): appends to the
    /// context.
    pub fn queue_add_context(&mut self, tracks: Vec<modplayer_audio_source::TrackRef>) {
        self.queue.ensure_host_driven();
        self.queue.add_context(tracks);
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
        self.drain_engine_events();
        // A `sync_program` send debounced on a previous call becomes
        // sendable once enough real time has passed, even with no further
        // mutation (contracts/transport-and-queue.md §3).
        self.flush_pending_program();
        self.flush_transfer_timer();
        self.flush_reconnect_timer();
        self.drain_library_load();
        self.tick_search();
        self.tick_library();
        self.flush_persistence();
        self.flush_track_state_if_due();
        self.drain_marker_store_events();
        self.analysis.drain();
    }

    /// The current track's latest waveform analysis, if any (contracts/
    /// analysis-service.md §1, contracts/transport-delta.md §2).
    pub fn analysis(&self) -> Option<&Arc<AnalysisSnapshot>> {
        self.analysis.latest()
    }

    /// The current track's marker/loop-region model (read-only
    /// projection for the UI), if any (006, contracts/marker-service.md
    /// §1).
    pub fn markers(&self) -> Option<&TrackMarkers> {
        self.markers.as_ref()
    }

    /// The resolved track-state directory, if any (tests/diagnostics —
    /// contracts/marker-service.md §1).
    pub fn track_state_dir(&self) -> Option<&std::path::Path> {
        self.track_state_paths.as_ref().map(|p| p.dir())
    }

    /// Snapshot of the armed region's real-time state, read from
    /// `RtShared` (006, contracts/marker-service.md §1).
    pub fn loop_status(&self) -> LoopStatus {
        LoopStatus {
            state: match self.shared.loop_state() {
                2 => LoopState::ArmedActive,
                1 => LoopState::ArmedInactive,
                _ => LoopState::Disarmed,
            },
            wraps: self.shared.loop_wraps(),
        }
    }

    /// The current track's marker model, creating a fresh (in-memory)
    /// one on first use or whenever it doesn't already match the current
    /// track (006, contracts/marker-service.md §1). `NoTrack` when
    /// nothing is queued. Persistence (US2's `sync_marker_attachment`)
    /// replaces this lazy-create with a real load/save cycle; this phase
    /// (US1) only needs a model to mutate for the current track.
    fn current_markers_mut(&mut self) -> Result<&mut TrackMarkers, MarkerError> {
        let item = self.current_track().ok_or(MarkerError::NoTrack)?;
        let id = item.track.id.clone();
        let len_frames = self.ms_to_frames(item.track.duration_ms);
        let rate = self.source_sample_rate.max(1);
        let needs_new = !matches!(&self.markers, Some(m) if m.track() == &id);
        if needs_new {
            self.markers = Some(TrackMarkers::new(id, rate, len_frames));
        }
        self.markers.as_mut().ok_or(MarkerError::NoTrack)
    }

    /// `I`/FR-006: move (or create) the current region's `A` endpoint to
    /// the playhead; re-commits the engine (without resetting wraps) if
    /// the region is armed (I3 swap handled inside `TrackMarkers`).
    pub fn set_loop_a(&mut self) -> Result<(), MarkerError> {
        let pos = self.shared.position_frames();
        let region = self.current_markers_mut()?.set_loop_a(pos)?.0;
        self.recommit_if_armed(region);
        Ok(())
    }

    /// As `set_loop_a`, for the `B` endpoint (`O`).
    pub fn set_loop_b(&mut self) -> Result<(), MarkerError> {
        let pos = self.shared.position_frames();
        let region = self.current_markers_mut()?.set_loop_b(pos)?.0;
        self.recommit_if_armed(region);
        Ok(())
    }

    /// A new, empty, incomplete region becomes current (contracts/
    /// marker-service.md §1: "list action").
    pub fn new_loop_region(&mut self) -> Result<RegionId, MarkerError> {
        Ok(self.current_markers_mut()?.new_loop_region())
    }

    /// Arm `region`: I6/I7, resets wraps, derives and pushes the
    /// engine's four setters + `LoopCommit { reset_wraps: true }`
    /// (contracts/marker-service.md §2), and requests the source cache
    /// the seam's incoming edge ahead of the playhead (FR-008).
    pub fn arm_loop(&mut self, region: RegionId) -> Result<(), MarkerError> {
        let rate = self.source_sample_rate.max(1);
        let (a, b, crossfade_ms, repeat, x) = {
            let markers = self.current_markers_mut()?;
            markers.arm(region)?;
            let r = markers.region(region).ok_or(MarkerError::NotFound)?;
            let (a, b) = r.span(markers).ok_or(MarkerError::RegionIncomplete)?;
            let x = r.effective_crossfade_frames(markers, rate);
            (a, b, r.crossfade_ms, r.repeat, x)
        };
        self.push_loop_engine_commands(a, b, crossfade_ms, repeat, true);
        self.source_host.command(SourceCommand::PrefetchHint {
            frame: a.saturating_sub(x),
        });
        Ok(())
    }

    /// `L` on an armed region (or the header's arm toggle): pushes
    /// `LoopDisarm` (contracts/marker-service.md §2). A seam already in
    /// flight on the engine still finishes for audio continuity.
    pub fn disarm_loop(&mut self) -> Result<(), MarkerError> {
        self.current_markers_mut()?.disarm();
        self.push_command_retrying(Command::LoopDisarm);
        Ok(())
    }

    /// `L`: arm `current_region` if it is disarmed, else disarm it
    /// (FR-007/FR-008).
    pub fn toggle_current_loop(&mut self) -> Result<(), MarkerError> {
        let region = self
            .current_markers_mut()?
            .current_region()
            .ok_or(MarkerError::RegionIncomplete)?;
        let armed_now = self
            .markers
            .as_ref()
            .and_then(|m| m.region(region))
            .map(|r| r.armed)
            .unwrap_or(false);
        if armed_now {
            self.disarm_loop()
        } else {
            self.arm_loop(region)
        }
    }

    /// Set `region`'s repeat count; an armed region re-commits without
    /// resetting wraps (contracts/marker-service.md §1).
    pub fn set_loop_repeat(
        &mut self,
        region: RegionId,
        repeat: RepeatCount,
    ) -> Result<(), MarkerError> {
        self.current_markers_mut()?.set_repeat(region, repeat)?;
        self.recommit_if_armed(region);
        Ok(())
    }

    /// Set `region`'s crossfade in milliseconds (`0..=50`); an armed
    /// region re-commits without resetting wraps.
    pub fn set_loop_crossfade_ms(&mut self, region: RegionId, ms: u8) -> Result<(), MarkerError> {
        self.current_markers_mut()?.set_crossfade_ms(region, ms)?;
        self.recommit_if_armed(region);
        Ok(())
    }

    // -- Non-loop marker mutations (006 US3, contracts/marker-service.md §1) --

    /// `M`: a point marker at the playhead (FR-001).
    pub fn add_point_marker(&mut self) -> Result<MarkerId, MarkerError> {
        let pos = self.shared.position_frames();
        self.current_markers_mut()?.add_point(pos)
    }

    /// `F2`/`Enter` commit (contracts/ui-markers.md §3): trims/truncates to
    /// 64 chars; empty on a `Point` keeps the old name.
    pub fn rename_marker(&mut self, id: MarkerId, name: &str) -> Result<(), MarkerError> {
        self.current_markers_mut()?.rename(id, name)
    }

    /// The colour-swatch popup's direct pick.
    pub fn recolor_marker(&mut self, id: MarkerId, color: PaletteIndex) -> Result<(), MarkerError> {
        self.current_markers_mut()?.recolor(id, color)
    }

    /// `C`: the next colour in the 8-entry palette, wrapping.
    pub fn cycle_marker_color(&mut self, id: MarkerId) -> Result<(), MarkerError> {
        self.current_markers_mut()?.cycle_color(id)
    }

    /// The region `id` belongs to, if it is a `RegionStart`/`RegionEnd`
    /// marker (for re-committing an armed region after `move_marker`/
    /// `delete_marker` — the region id itself is unaffected by I3's
    /// endpoint-kind swap, so this is safe to read *before* the mutation).
    fn marker_region(&self, id: MarkerId) -> Option<RegionId> {
        match self.markers.as_ref()?.marker(id)?.kind {
            MarkerKind::RegionStart { region } | MarkerKind::RegionEnd { region } => Some(region),
            MarkerKind::Point | MarkerKind::Cue { .. } => None,
        }
    }

    /// Drag/nudge commit (FR-019 clamp inside the model); re-commits an
    /// armed region without resetting wraps if `id` is one of its
    /// endpoints.
    pub fn move_marker(&mut self, id: MarkerId, frame: u64) -> Result<(), MarkerError> {
        let region = self.marker_region(id);
        self.current_markers_mut()?.move_marker(id, frame)?;
        if let Some(region) = region {
            self.recommit_if_armed(region);
        }
        Ok(())
    }

    /// `nudge_step_ms()` converted to source-rate frames (0 before any
    /// stream has ever opened, matching `ms_to_frames`).
    fn nudge_step_frames(&self) -> u64 {
        self.ms_to_frames(u32::from(self.nudge_step_ms))
    }

    /// `←`/`→` (`multiplier = 1`) / `Shift+←`/`Shift+→` (`multiplier =
    /// 10`, contracts/ui-markers.md §3): `frame ± nudge_step_frames ×
    /// multiplier`, saturating at `0`/`len_frames` (the model's own I4
    /// clamp).
    pub fn nudge_marker(
        &mut self,
        id: MarkerId,
        direction: i8,
        multiplier: u8,
    ) -> Result<(), MarkerError> {
        let step = self
            .nudge_step_frames()
            .saturating_mul(u64::from(multiplier));
        let current = self
            .markers
            .as_ref()
            .and_then(|markers| markers.position_of(id))
            .ok_or(MarkerError::NotFound)?;
        let target = if direction < 0 {
            current.saturating_sub(step)
        } else {
            current.saturating_add(step)
        };
        self.move_marker(id, target)
    }

    /// `Delete`/`Backspace` (I9): disarms the engine (not just the model)
    /// if `id` was an endpoint of the currently armed region.
    pub fn delete_marker(&mut self, id: MarkerId) -> Result<(), MarkerError> {
        let was_armed_endpoint = self.markers.as_ref().is_some_and(|markers| {
            markers
                .armed_region()
                .is_some_and(|r| r.a == Some(id) || r.b == Some(id))
        });
        self.current_markers_mut()?.delete(id)?;
        if was_armed_endpoint {
            self.push_command_retrying(Command::LoopDisarm);
        }
        Ok(())
    }

    /// `I8` only (no engine effect): a click or `Tab` onto a glyph/row.
    pub fn select_marker(&mut self, id: MarkerId) -> Result<(), MarkerError> {
        self.current_markers_mut()?.select_marker(id)
    }

    // -- Cue points (006 US4, contracts/marker-service.md §1) --------------

    /// `Shift+1..8`: set (or move) `slot`'s cue to the playhead (FR-013,
    /// I5/I1).
    pub fn set_cue(&mut self, slot: CueSlot) -> Result<MarkerId, MarkerError> {
        let pos = self.shared.position_frames();
        self.current_markers_mut()?.set_cue(slot, pos)
    }

    /// `1..8`: jump to `slot`'s cue through 005's existing sample-accurate
    /// seek path (`seek_frames`), never altering play/pause state
    /// (FR-014 — the reducer's existing `Input::Seek` behaviour already
    /// leaves transport state untouched). `false` (a silent no-op) on an
    /// empty slot.
    pub fn jump_to_cue(&mut self, slot: CueSlot) -> bool {
        let Some(pos) = self
            .markers
            .as_ref()
            .and_then(|m| m.cue(slot))
            .map(|c| c.position)
        else {
            return false;
        };
        self.seek_frames(pos);
        true
    }

    /// If `region` is currently armed, re-derive and push the engine's
    /// setters + `LoopCommit { reset_wraps: false }` — an edit-while-
    /// armed (contracts/marker-service.md §1-§2). A no-op otherwise
    /// (including when there is no current track/region at all).
    fn recommit_if_armed(&mut self, region: RegionId) {
        let data = self.markers.as_ref().and_then(|markers| {
            let r = markers.region(region)?;
            if !r.armed {
                return None;
            }
            let (a, b) = r.span(markers)?;
            Some((a, b, r.crossfade_ms, r.repeat))
        });
        if let Some((a, b, crossfade_ms, repeat)) = data {
            self.push_loop_engine_commands(a, b, crossfade_ms, repeat, false);
        }
    }

    /// Derive and push the engine's loop `Command`s for `[a, b)`
    /// (contracts/marker-service.md §2): setters first, `LoopCommit`
    /// last, always through `push_command_retrying` so a momentarily
    /// full queue preserves order.
    fn push_loop_engine_commands(
        &mut self,
        a: u64,
        b: u64,
        crossfade_ms: u8,
        repeat: RepeatCount,
        reset_wraps: bool,
    ) {
        let rate = self.source_sample_rate.max(1);
        let crossfade_frames = configured_crossfade_frames(crossfade_ms, rate);
        let repeat_u32 = match repeat {
            RepeatCount::Infinite => 0,
            RepeatCount::Times(n) => u32::from(n),
        };
        self.push_command_retrying(Command::LoopSetA(a));
        self.push_command_retrying(Command::LoopSetB(b));
        self.push_command_retrying(Command::LoopSetSeam {
            crossfade_frames,
            repeat: repeat_u32,
        });
        self.push_command_retrying(Command::LoopCommit { reset_wraps });
    }

    /// The armed region's `A` position in frames, if any.
    fn armed_region_a_frame(&self) -> Option<u64> {
        let markers = self.markers.as_ref()?;
        let region = markers.armed_region()?;
        region.span(markers).map(|(a, _b)| a)
    }

    /// Drain the running `Processor`'s events (006, contracts/marker-
    /// service.md §4): >= 1 `LoopWrapped` this tick sends one coalesced,
    /// throttled `SourceCommand::Seek` so the streaming `Player` follows
    /// the loop (research R5); `LoopReleased` disarms the *model* (the RT
    /// already stopped looping on its own). `ToneFinished`/`TrackLooped`/
    /// `CommandDropped` are unchanged (ignored).
    fn drain_engine_events(&mut self) {
        let Some(rx) = self.event_rx.as_mut() else {
            return;
        };
        let mut wrapped = false;
        let mut released = false;
        while let Ok(event) = rx.pop() {
            match event {
                Event::LoopWrapped { .. } => wrapped = true,
                Event::LoopReleased { .. } => released = true,
                Event::ToneFinished | Event::TrackLooped { .. } | Event::CommandDropped { .. } => {}
            }
        }
        if wrapped {
            self.reseek_for_loop_wrap();
        }
        if released && let Some(markers) = self.markers.as_mut() {
            markers.disarm();
        }
    }

    /// One coalesced `SourceCommand::Seek(A)` per tick with a wrap,
    /// throttled to <= 1 per `LOOP_RESEEK_INTERVAL` after the first
    /// (research R5).
    fn reseek_for_loop_wrap(&mut self) {
        let Some(a_frame) = self.armed_region_a_frame() else {
            return;
        };
        let now = (self.now)();
        let due = self
            .last_loop_reseek_at
            .is_none_or(|last| now.saturating_duration_since(last) >= LOOP_RESEEK_INTERVAL);
        if !due {
            return;
        }
        self.last_loop_reseek_at = Some(now);
        let position_ms = if self.source_sample_rate == 0 {
            0
        } else {
            u32::try_from((a_frame * 1000) / u64::from(self.source_sample_rate)).unwrap_or(u32::MAX)
        };
        self.source_host.command(SourceCommand::Seek(position_ms));
    }

    /// Detach/reattach the Analysis Service whenever `queue.current()`
    /// actually changed since the last check — covers every path a
    /// current-track change can take (skip, end of track, queue replace,
    /// unavailable skip, transfer-out) with one check rather than one per
    /// `Input` variant (contracts/transport-delta.md §2). `stop()` never
    /// changes `queue.current()`, so this is naturally a no-op for it
    /// (FR-017: a stopped-with-current-track keeps its waveform).
    fn sync_analysis_attachment(&mut self) {
        let current_id = self.queue.current().map(|item| item.track.id.clone());
        if current_id == self.last_analysis_track {
            return;
        }
        self.analysis.detach();
        if let Some(item) = self.queue.current() {
            let len_frames = self.ms_to_frames(item.track.duration_ms);
            self.analysis
                .attach(item.track.id.clone(), self.source_sample_rate, len_frames);
        }
        self.last_analysis_track = current_id;
    }

    /// Flush/disarm/(re)load the marker/loop-region model whenever
    /// `queue.current()`'s id changed since the last sync, **or** the
    /// same id was just re-started (`restarted`, from `Input::
    /// TrackStarted` — FR-016's "same-session reload"; unlike
    /// `sync_analysis_attachment`'s change-only check, contracts/
    /// marker-service.md §4): the previous track's dirty state is
    /// flushed, the engine is disarmed, the new track's state is loaded
    /// (raising any warning at most once per load), and the model is
    /// seeded with the *current* (pre-`TrackStarted`) track length —
    /// `dispatch`'s `Input::TrackStarted` handling (T058) refines that
    /// once the authoritative length is known.
    fn sync_marker_attachment(&mut self, restarted: bool) {
        let current = self.queue.current().cloned();
        let current_id = current.as_ref().map(|item| item.track.id.clone());
        let changed = current_id != self.marker_state_track;
        if !(changed || (restarted && current_id.is_some())) {
            return;
        }
        if !changed {
            // A `TrackStarted` for the attachment we just made on the id
            // change: the state is already loaded, so reloading here would
            // only repeat the work and re-raise its warning. Consume the
            // expectation — the *next* `TrackStarted` is a real restart.
            if self.marker_state_awaiting_start {
                self.marker_state_awaiting_start = false;
                return;
            }
        } else {
            // `restarted` here means this very `TrackStarted` is what made
            // the id change, so it is already accounted for.
            self.marker_state_awaiting_start = !restarted;
        }
        self.flush_track_state();
        self.push_command_retrying(Command::LoopDisarm);
        self.markers = None;
        self.marker_state_track = current_id;
        let Some(item) = current else {
            return;
        };
        let rate = self.source_sample_rate.max(1);
        let len_frames = self.ms_to_frames(item.track.duration_ms);
        let outcome = match &self.track_state_paths {
            Some(paths) => markers::store::load(paths, &item.track.id, rate, len_frames),
            None => markers::store::LoadOutcome {
                state: TrackMarkers::new(item.track.id.clone(), rate, len_frames),
                warning: None,
                rewrite_allowed: true,
            },
        };
        if let Some(warning) = outcome.warning {
            let key = match warning {
                markers::store::LoadWarning::Unreadable => KEY_TRACK_STATE_UNREADABLE,
                markers::store::LoadWarning::NewerSchema => KEY_TRACK_STATE_NEWER_VERSION,
            };
            self.notifications.raise(Severity::Warning, key);
        }
        self.markers = Some(outcome.state);
    }

    /// Force-flush the current track's marker/loop-region state if dirty
    /// (track change, sign-out, shutdown, `clear_all_markers` — research
    /// R11); a no-op with nothing dirty or no resolvable track-state
    /// directory.
    fn flush_track_state(&mut self) {
        let Some(paths) = &self.track_state_paths else {
            return;
        };
        let Some(markers) = self.markers.as_mut() else {
            return;
        };
        if !markers.is_dirty() {
            return;
        }
        let path = paths.file_for(markers.track());
        let bytes = markers::store::encode(markers);
        markers.mark_clean();
        if let Some(tx) = &self.marker_persist_tx {
            let _ = tx.send(markers::store::PersistJob::Save { path, bytes });
        }
    }

    /// Debounced flush, checked every `tick()` (research R11,
    /// `TRACK_STATE_DEBOUNCE`): the last mutation before the window
    /// elapses wins.
    fn flush_track_state_if_due(&mut self) {
        let Some(markers) = self.markers.as_ref() else {
            return;
        };
        if !markers.is_dirty() {
            return;
        }
        let now = (self.now)();
        let due = self
            .last_marker_flush_at
            .is_none_or(|last| now.saturating_duration_since(last) >= TRACK_STATE_DEBOUNCE);
        if !due {
            return;
        }
        self.last_marker_flush_at = Some(now);
        self.flush_track_state();
    }

    /// Drain the background writer's failure replies (contracts/
    /// marker-service.md §3 rule 1): each raises `track-state-save-failed`
    /// once — the previous file, if any, was left intact and there is no
    /// automatic retry.
    fn drain_marker_store_events(&mut self) {
        let Some(rx) = &self.marker_store_rx else {
            return;
        };
        let mut failed = false;
        while let Ok(event) = rx.try_recv() {
            match event {
                markers::store::StoreEvent::SaveFailed { .. } => failed = true,
            }
        }
        if failed {
            self.notifications
                .raise(Severity::Warning, KEY_TRACK_STATE_SAVE_FAILED);
        }
    }

    /// "Clear all markers" (contracts/marker-service.md §1): disarms the
    /// engine if a region was armed, empties the model, and flushes
    /// immediately — the empty state is written straight away rather than
    /// waiting out the debounce.
    pub fn clear_all_markers(&mut self) {
        let was_armed = self
            .markers
            .as_ref()
            .is_some_and(|m| m.armed_region().is_some());
        if was_armed {
            self.push_command_retrying(Command::LoopDisarm);
        }
        if let Some(markers) = self.markers.as_mut() {
            markers.clear_all();
        }
        self.flush_track_state();
    }

    /// Sign-out ordering (contracts/marker-service.md §4): flush, drop the
    /// in-memory state (markers are not account data — never deleted, per
    /// FR-024/data-model.md §4 rule 4), disarm the engine.
    fn clear_marker_state_for_sign_out(&mut self) {
        self.flush_track_state();
        self.markers = None;
        self.marker_state_track = None;
        self.marker_state_awaiting_start = false;
        self.push_command_retrying(Command::LoopDisarm);
    }

    /// Drive `SearchSession`'s debounce timer every tick
    /// (004-search-and-library-browse, contracts/library-and-search-
    /// core.md §1): reports the derived online fact, then forwards
    /// whatever `SourceCommand`s the debounce deadline elapsing produces.
    fn tick_search(&mut self) {
        let online = self.is_online();
        self.search.set_offline(!online);
        let now = (self.now)();
        for cmd in self.search.tick(now) {
            self.source_host.command(cmd);
        }
    }

    /// Persistence-thread hook point (004-search-and-library-browse,
    /// contracts/library-and-search-core.md §1: "flushes dirty persistence
    /// <= 1 s later on the persistence thread"): sends a dirty
    /// `LibraryIndex`/`PlayLog` snapshot to the background writer at most
    /// once per `PERSIST_DEBOUNCE`. A no-op when nothing is dirty, or when
    /// no data directory was resolvable at construction (`persist_tx`).
    fn flush_persistence(&mut self) {
        let Some(tx) = &self.persist_tx else {
            return;
        };
        if !self.library_dirty && !self.play_log.is_dirty() {
            return;
        }
        let now = (self.now)();
        let due = self
            .last_persist_flush_at
            .is_none_or(|last| now.saturating_duration_since(last) >= PERSIST_DEBOUNCE);
        if !due {
            return;
        }
        if self.library_dirty {
            let _ = tx.send(PersistJob::SaveIndex(Box::new(self.library.clone())));
            self.library_dirty = false;
        }
        if self.play_log.is_dirty() {
            let _ = tx.send(PersistJob::SavePlayLog(Box::new(self.play_log.clone())));
            self.play_log.mark_clean();
        }
        self.last_persist_flush_at = Some(now);
    }

    /// Drain the background-load reply, once (004-search-and-library-
    /// browse, research R6): replaces the empty in-memory `library`/
    /// `play_log` with whatever was on disk. A no-op once drained (or if
    /// no paths were resolvable at construction).
    fn drain_library_load(&mut self) {
        let Some(rx) = &self.load_rx else {
            return;
        };
        match rx.try_recv() {
            Ok((index_outcome, log_outcome)) => {
                self.library = index_outcome.index;
                self.play_log = log_outcome.play_log;
                self.library_loading = false;
                self.load_rx = None;
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.library_loading = false;
                self.load_rx = None;
            }
        }
    }

    /// Drive the `FetchLibrary` cycle scheduler and the lazy hydration
    /// sweep every tick (004-search-and-library-browse, research R4/R6,
    /// design note 7). `Launch`/`Reconnect` (data-model.md §3.3) are both
    /// exactly the offline -> online edge: the very first online tick is
    /// Launch, any later one is Reconnect (FR-011's "immediate sync").
    fn tick_library(&mut self) {
        // `MODPLAYER_LIBRARY_FIXTURE=large` (quickstart M15, US3 T072):
        // seed the synthetic 50 000/1 000 fixture once *instead of* the
        // real library, so the at-scale experience can be rehearsed
        // without a real account that large. While the override is set,
        // neither the on-disk snapshot (`load_rx` dropped) nor the sync
        // cycle (returned before the scheduler) may replace or merge over
        // it — the 2026-09-17 manual walk found both doing exactly that
        // once persistence was wired in. Deliberately never persisted (no
        // `mark_library_dirty`) — a debug rehearsal aid, not real account
        // data — so it never lands in a real user's `index.json`.
        #[cfg(debug_assertions)]
        if crate::library::index::large_fixture_requested() {
            if !self.large_fixture_seeded {
                self.library = crate::library::index::large_fixture();
                self.large_fixture_seeded = true;
                self.library_loading = false;
                self.load_rx = None;
            }
            return;
        }

        let now = (self.now)();
        let online = self.is_online();
        let reconnected = online && !self.was_online_for_sync;
        self.was_online_for_sync = online;

        let commands = if reconnected {
            self.sync.trigger(now)
        } else {
            self.sync.tick(now, online)
        };
        for cmd in commands {
            self.source_host.command(cmd);
        }
        self.maybe_sweep_hydration(now);
    }

    /// Issue one bounded `HydrateRefs` sweep request if the throttle
    /// window has elapsed and anything is still queued (contracts/
    /// library-and-search-core.md §4: `HYDRATE_BATCH` = 100, <= 2
    /// batches/s).
    fn maybe_sweep_hydration(&mut self, now: Instant) {
        if !self.library.has_pending_hydration() {
            return;
        }
        let due = self
            .last_hydration_sweep_at
            .is_none_or(|last| now.saturating_duration_since(last) >= HYDRATE_SWEEP_INTERVAL);
        if !due {
            return;
        }
        let batch = self.library.drain_hydration_batch(HYDRATE_BATCH);
        if batch.is_empty() {
            return;
        }
        self.last_hydration_sweep_at = Some(now);
        let request_id = self.next_library_request_id;
        self.next_library_request_id += 1;
        self.source_host.command(SourceCommand::HydrateRefs {
            request_id,
            tracks: batch.tracks,
            albums: batch.albums,
            artists: batch.artists,
        });
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
            // Catalog events (004-search-and-library-browse, contracts/
            // library-and-search-core.md §1) route by `request_id` to
            // search/library state, **never** to the transport reducer
            // (design note 1's "Seam first, UI last").
            match event {
                SourceEvent::SearchResult { request_id, result } => {
                    self.route_search_result(request_id, result);
                    continue;
                }
                SourceEvent::LibraryPage { request_id, result } => {
                    self.route_library_page(request_id, result);
                    continue;
                }
                SourceEvent::TrackList { request_id, result } => {
                    self.route_track_list(request_id, result);
                    continue;
                }
                SourceEvent::Hydrated {
                    request_id,
                    tracks,
                    albums,
                    artists,
                    missing,
                } => {
                    self.route_hydrated(request_id, tracks, albums, artists, missing);
                    continue;
                }
                SourceEvent::DecodedStore { track, store } => {
                    // Routed directly rather than through the reducer
                    // (contracts/transport-delta.md §2): `attach_store`
                    // itself ignores a store for a non-current track.
                    self.analysis.attach_store(track, store);
                    continue;
                }
                // 006, research R5 rule 3: a region ends at `b < len`, so
                // the RT never legitimately reaches its own end of track
                // while a loop is armed-active — ignore it here rather
                // than let the queue advance mid-loop; the controller's
                // throttled re-seek (`reseek_for_loop_wrap`) keeps the
                // streaming `Player` inside the region instead.
                SourceEvent::EndOfTrack if self.shared.loop_state() == 2 => continue,
                _ => {}
            }
            if let Some(input) = map_source_event(event) {
                self.dispatch(input);
            }
        }
    }

    /// Route a `SearchResult` reply into `SearchSession` by `request_id`
    /// (contracts/library-and-search-core.md §1, US1 T035):
    /// `SearchSession::apply_reply` itself drops a stale-generation or
    /// while-offline reply.
    #[allow(clippy::needless_pass_by_value)]
    fn route_search_result(
        &mut self,
        request_id: u64,
        result: Result<
            modplayer_audio_source::catalog::SearchPage,
            modplayer_audio_source::catalog::CatalogError,
        >,
    ) {
        self.search.apply_reply(request_id, result);
    }

    /// Route a `LibraryPage` reply into `SyncScheduler`/`LibraryIndex`
    /// (contracts/library-and-search-core.md §1, US2 T060): merges the
    /// page, forwards whatever follow-up commands the scheduler's own
    /// page-walking produces, and — once a whole cycle completes — records
    /// its outcome (data-model.md §3.2/§3.3). `last_synced_at` only
    /// advances on a non-`Failed` outcome, so a later failed cycle never
    /// erases the timestamp of the last one that actually landed
    /// (`SyncMeta::first_sync_failed` stays correct for FR-021).
    #[allow(clippy::needless_pass_by_value)]
    fn route_library_page(&mut self, request_id: u64, result: Result<LibraryPage, CatalogError>) {
        let now = (self.now)();
        let outcome = self.sync.apply_reply(now, request_id, result);
        if let Some(page) = outcome.merged_page {
            self.library.merge_page(page);
            self.mark_library_dirty();
        }
        for cmd in outcome.commands {
            self.source_host.command(cmd);
        }
        if let Some(cycle_outcome) = self.sync.take_last_outcome() {
            self.library.meta.last_outcome = cycle_outcome;
            if !matches!(cycle_outcome, SyncOutcome::Failed) {
                let now_ms = unix_ms_now();
                self.library.meta.last_synced_at = Some(now_ms);
            }
            self.mark_library_dirty();
        }
    }

    /// Route a `TrackList` reply (contracts/library-and-search-core.md §1
    /// `library_track_list`): caches the ordered ref list on success;
    /// either way, clears this source's in-flight marker so a later call
    /// can retry.
    #[allow(clippy::needless_pass_by_value)]
    fn route_track_list(&mut self, request_id: u64, result: Result<TrackList, CatalogError>) {
        self.track_list_in_flight.retain(|_, id| *id != request_id);
        if let Ok(list) = result {
            self.library.merge_track_list(list);
            self.mark_library_dirty();
        }
    }

    /// Route a `Hydrated` reply (contracts/library-and-search-core.md §1;
    /// contracts/catalog-source.md §2 rule 7): merges every resolved
    /// entity and maps a missing track uri to `Availability::Removed`
    /// (`LibraryIndex::apply_hydrated`).
    #[allow(clippy::needless_pass_by_value, clippy::too_many_arguments)]
    fn route_hydrated(
        &mut self,
        _request_id: u64,
        tracks: Vec<TrackRef>,
        albums: Vec<modplayer_audio_source::catalog::AlbumRef>,
        artists: Vec<modplayer_audio_source::catalog::ArtistRef>,
        missing: Vec<String>,
    ) {
        self.library
            .apply_hydrated(tracks, albums, artists, missing);
        self.mark_library_dirty();
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
        // research R11 (004-search-and-library-browse): a `PlayLog` entry
        // is recorded only on a *source-confirmed* start — `Input::Playing`
        // (the source's own confirmation, however quickly it follows the
        // optimistic local `Input::Play`) or a transfer-in that arrives
        // already `playing` (`Input::BecameActive { context: Some(ctx) }`
        // with `ctx.playing`). Deliberately **not** `Input::Play` itself:
        // that dispatch flips `intent` to `Playing` optimistically before
        // the source has had any chance to reply — for a track the source
        // is about to refuse (`Input::Unavailable` arrives on the very next
        // `poll()`), that optimistic instant is the *only* moment
        // `is_actively_playing` is briefly true, so gating on it would
        // record a track that never actually played (FR-012's "unavailable-
        // skipped not recorded"). `record_play_log_if_new_track`'s own
        // dedup (`last_recorded_play_track`) still applies, so a resume's
        // confirming `Input::Playing` for the same track that is already
        // recorded is a no-op.
        let confirms_playing = matches!(&input, Input::Playing)
            || matches!(&input, Input::BecameActive { context: Some(ctx) } if ctx.playing);
        // 006, contracts/marker-service.md §4: a `TrackStarted` for the
        // *same* current-track id is a same-session reload (FR-016) that
        // `sync_marker_attachment` must still flush/reload for, unlike
        // `sync_analysis_attachment`'s change-only check; `track_len_ms`
        // is the authoritative length T058 re-clamps the model to, once
        // that model exists.
        let track_started_len_ms = if let Input::TrackStarted { track_len_ms, .. } = &input {
            Some(*track_len_ms)
        } else {
            None
        };
        let (new_state, effects) =
            transport::reduce(std::mem::take(&mut self.transport_state), input);
        self.transport_state = new_state;
        // Applied *after* `apply_effects`: a transfer-in
        // (`Effect::Queue(AdoptTransferContext)`) sets the queue's
        // `current()` as an *effect* of this same `reduce` call, not before
        // it — recording immediately after `reduce` would log whatever
        // track was current beforehand instead of the one that actually
        // started playing.
        self.apply_effects(effects, queue_origin);
        if confirms_playing && is_actively_playing(&self.transport_state) {
            self.record_play_log_if_new_track();
        }
        self.sync_analysis_attachment();
        self.sync_marker_attachment(track_started_len_ms.is_some());
        // T058: refine the model's `len_frames` to the engine-reported
        // length once it is known — may differ from the catalog metadata
        // `sync_marker_attachment` loaded against (FR-018 clamp + flag).
        if let Some(len_ms) = track_started_len_ms {
            let len_frames = self.ms_to_frames(len_ms);
            if let Some(markers) = self.markers.as_mut() {
                markers.set_len_frames(len_frames);
            }
        }
    }

    /// research R11: record a `PlayLog` entry on the `TrackStarted ->
    /// Playing` transition, for a track id that differs from the last one
    /// recorded — never for the same track the transport was already
    /// playing (a plain pause/resume), and never for a track that reached
    /// `Unavailable` instead of `Playing` (FR-012's Connect-transfer and
    /// unavailable-skip rules both fall out of this for free: a transfer-in
    /// still lands on this same `TrackStarted -> Playing` path; an
    /// unavailable track never does).
    fn record_play_log_if_new_track(&mut self) {
        let Some(current) = self.queue.current() else {
            return;
        };
        if self.last_recorded_play_track.as_ref() == Some(&current.track.id) {
            return;
        }
        self.last_recorded_play_track = Some(current.track.id.clone());
        // `play_log.record` sets its own internal dirty flag — `flush_
        // persistence` checks it independently of `library_dirty`.
        self.play_log.record(&current.track, unix_ms_now());
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
                Effect::SeekTo {
                    position_ms,
                    position_frames,
                } => {
                    let frames = position_frames.unwrap_or_else(|| self.ms_to_frames(position_ms));
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
                // 006, research R6: a freshly built `Processor` starts
                // disarmed — re-push the armed region's four setters +
                // `LoopCommit { reset_wraps: false }` so the loop survives
                // this rebuild with its wrap count intact.
                let repush = self.markers.as_ref().and_then(|markers| {
                    let region = markers.armed_region()?;
                    let (a, b) = region.span(markers)?;
                    Some((a, b, region.crossfade_ms, region.repeat))
                });
                if let Some((a, b, crossfade_ms, repeat)) = repush {
                    self.push_loop_engine_commands(a, b, crossfade_ms, repeat, false);
                }
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

/// research R11: "`Active{intent: Playing, buffering: false}`" — the exact
/// transport state the play-log record hook edge-detects a transition
/// into.
fn is_actively_playing(state: &TransportState) -> bool {
    state.intent == Intent::Playing && !state.buffering
}

/// Current wall time in unix milliseconds, clamped to 0 if the clock is
/// somehow set before the epoch (never true in production). Persisted
/// timestamps (`PlayLog`, `SyncMeta`) are host-only bookkeeping, not the
/// injectable `now()` clock — real wall time either way.
fn unix_ms_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// `round(crossfade_ms * rate / 1000)` (contracts/marker-service.md §2):
/// the *configured* crossfade in source frames, rounded half up. The
/// engine itself further reduces this via `loop_math::effective_
/// crossfade` at the seam's start.
fn configured_crossfade_frames(crossfade_ms: u8, rate: u32) -> u32 {
    let scaled = u64::from(crossfade_ms) * u64::from(rate);
    u32::try_from((scaled + 500) / 1000).unwrap_or(u32::MAX)
}

fn severity_for(warning: &SettingsWarning) -> Severity {
    match warning {
        SettingsWarning::Unreadable
        | SettingsWarning::NewerVersion
        | SettingsWarning::InvalidValue(_)
        | SettingsWarning::InvalidKeybindings(_) => Severity::Warning,
    }
}

/// Raise one `LoadOutcome.warnings` entry as a notification (007,
/// contracts/keymap-settings.md: `PlaybackController::new` "raises each").
/// `InvalidKeybindings` carries the dropped action ids as its Fluent
/// argument; every other variant raises with no arguments as before.
fn raise_settings_warning(notifications: &mut NotificationCenter, warning: &SettingsWarning) {
    match warning {
        SettingsWarning::InvalidKeybindings(ids) => {
            notifications.raise_with_args(
                Severity::Warning,
                warning.message_key(),
                vec![("ids", ids.join(", "))],
            );
        }
        _ => {
            notifications.raise(severity_for(warning), warning.message_key());
        }
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

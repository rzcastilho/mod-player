// SPDX-License-Identifier: MIT OR Apache-2.0

//! `connect-worker`: the dedicated OS thread + tokio runtime that owns
//! librespot's `Session`/`Player`/`Spirc` for the lifetime of the receiver
//! (contracts/connect-source.md §2, §5). Spawned lazily on the first
//! `SourceCommand::Initialize`; everything librespot touches (network, the
//! dealer, the player's own decode thread) lives here, never on the UI
//! thread (design note 4).
//!
//! `Player` (and the `RingSink`/ring producer it owns) is built exactly
//! once, at the first successful connect: `RingSink`'s `rtrb::Producer` is
//! move-only, so it cannot be rebuilt on every reconnect. Session drops —
//! `Retry` or an unexpected end — instead rebuild `Session` + `Spirc` and
//! call `Player::set_session` to re-point the existing player, matching
//! how librespot's own CLI handles session loss. For the same reason a
//! `Deregister` parks the thread (session torn down, `Player` kept) until
//! the next `Initialize` rather than exiting it (#31).
//!
//! Scope note (US1): a single automatic backoff-and-reconnect loop covers
//! an unexpected session end; the richer session-loss / transient-health
//! UI treatment (30 s warning, session-event handling) is US4's job
//! (T086-T090) — this worker already emits the `Health` events that later
//! phase reduces on.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use librespot_connect::{
    ConnectConfig as SpircConnectConfig, LoadContextOptions, LoadRequest, LoadRequestOptions,
    Options, PlayingTrack, Spirc,
};
use librespot_core::SpotifyUri;
use librespot_core::authentication::Credentials;
use librespot_core::config::{DeviceType, SessionConfig};
use librespot_core::session::Session;
use librespot_playback::config::PlayerConfig;
use librespot_playback::mixer::{Mixer, NoOpVolume};
use librespot_playback::player::{Player, PlayerEvent};
use modplayer_audio_source::{
    CatalogError, DecodedStore, Program, SourceCommand, SourceEvent, SourceHealth, SourceRtShared,
};
use rtrb::{Consumer, Producer};

use crate::credentials::ReceiverCredentials;
use crate::decode_ahead::{self, DecodeAheadHandle, ForceFail};
use crate::events::{self, MapperState};
use crate::health::{self, Backoff};
use crate::mixer::HostMixer;
use crate::program::{Marker, ProgramMap};
use crate::sink::RingSink;
use crate::swap::RingSwap;

/// The decode-ahead thread for whatever track is current, shared between
/// the player-event task (which spawns a fresh one per `TrackChanged`, its
/// `Drop` stopping the previous one) and `command_loop` (which forwards
/// seek hints into it and stops it on `Stop`/`Shutdown`) — 005-now-
/// playing-waveform, contracts/connect-source-delta.md §1-2.
type SharedDecodeAhead = Arc<Mutex<Option<DecodeAheadHandle>>>;

/// Pop-and-drop every store the RT half has retired since the last call
/// (research R1's drop discipline: the RT never drops a store itself —
/// this is the one place, on the worker thread, a store's last `Arc` is
/// ever actually freed). Called once per `command_loop` iteration and
/// immediately before every `marker_tx.push` (contracts/connect-source-
/// delta.md §2).
fn drain_retired(retired_rx: &mut Consumer<Arc<DecodedStore>>) {
    while let Ok(store) = retired_rx.pop() {
        drop(store);
    }
}

/// The session's one `MapperState`, shared by the command loop and the
/// player-event task (see `apply_load_program`).
type SharedMapper = Arc<Mutex<MapperState>>;

/// The mapper holds no invariant a panic mid-update could break (two
/// `Option`s), so a poisoned lock is simply recovered.
fn lock_mapper(mapper: &SharedMapper) -> std::sync::MutexGuard<'_, MapperState> {
    mapper
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// How long the player-event task waits for a `TrackChanged` after an
/// unsolicited `SessionConnected` before falling back to
/// `BecameActive { context: None }` (contracts/connect-source.md §3,
/// T074).
const TRANSFER_CONTEXT_WINDOW: Duration = Duration::from_secs(1);

/// Static configuration the worker needs for the lifetime of the receiver
/// (contracts/connect-source.md §1's `ConnectConfig`, narrowed to what the
/// worker thread itself needs — `status_page_url` stays UI-only, design
/// note 10).
#[derive(Clone)]
pub struct WorkerConfig {
    pub device_id: String,
    pub tmp_dir: PathBuf,
    pub credentials: Arc<dyn ReceiverCredentials>,
    pub initial_volume_pct: u8,
}

pub struct WorkerHandles {
    cmd_tx: Sender<SourceCommand>,
    thread: Option<JoinHandle<()>>,
}

impl WorkerHandles {
    pub fn send(&self, cmd: SourceCommand) {
        // The worker thread only ever exits after `Shutdown`; a send
        // after that is a caller bug we simply ignore (there is no
        // reasonable recovery on a plain `mpsc::Sender`).
        let _ = self.cmd_tx.send(cmd);
    }

    /// Block until the worker thread has finished, up to `timeout`
    /// (contract §2: "synchronous best effort <= 2 s").
    pub fn join_with_timeout(&mut self, timeout: Duration) {
        let Some(handle) = self.thread.take() else {
            return;
        };
        let start = Instant::now();
        while !handle.is_finished() && start.elapsed() < timeout {
            thread::sleep(Duration::from_millis(10));
        }
        let _ = handle.join();
    }
}

/// Spawn the worker thread. Returns `None` on an OS-level failure to
/// spawn a thread (vanishingly rare); the caller reports `Health(
/// Unavailable)` in that case.
#[allow(clippy::too_many_arguments)]
pub fn spawn(
    device_name: String,
    config: WorkerConfig,
    event_tx: Sender<SourceEvent>,
    sample_tx: Producer<f32>,
    marker_tx: Producer<Marker>,
    retired_rx: Consumer<Arc<DecodedStore>>,
    shared: Arc<SourceRtShared>,
    swap: Arc<RingSwap>,
    initial_program: Option<Program>,
) -> Option<WorkerHandles> {
    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<SourceCommand>();
    // Read once at worker spawn (contracts/connect-source-delta.md §1);
    // never affects the `Player`.
    let force_fail = ForceFail::from_env();
    let builder = thread::Builder::new().name("connect-worker".to_string());
    let spawned = builder.spawn(move || {
        run(
            device_name,
            config,
            cmd_rx,
            event_tx,
            sample_tx,
            marker_tx,
            retired_rx,
            shared,
            swap,
            initial_program,
            force_fail,
        )
    });
    match spawned {
        Ok(thread) => Some(WorkerHandles {
            cmd_tx,
            thread: Some(thread),
        }),
        Err(_) => None,
    }
}

// The `ConnectWorker` thread's entry point owns every long-lived handle a
// librespot session needs (channels to/from the RT half, the pending-
// program carried across a restart); splitting it into a params struct
// would obscure ownership more than it clarifies argument count.
#[allow(clippy::too_many_arguments)]
fn run(
    mut device_name: String,
    config: WorkerConfig,
    cmd_rx: Receiver<SourceCommand>,
    event_tx: Sender<SourceEvent>,
    sample_tx: Producer<f32>,
    mut marker_tx: Producer<Marker>,
    mut retired_rx: Consumer<Arc<DecodedStore>>,
    shared: Arc<SourceRtShared>,
    swap: Arc<RingSwap>,
    mut pending_program: Option<Program>,
    force_fail: ForceFail,
) {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => {
            report_health(
                &event_tx,
                SourceHealth::Unavailable {
                    client_update_required: false,
                },
            );
            return;
        }
    };

    let written_frames = Arc::new(AtomicU64::new(0));
    let mixer = Arc::new(HostMixer::new(config.initial_volume_pct));
    // 005-now-playing-waveform: the current track's decode-ahead thread,
    // shared across reconnects (a session loss never touches it —
    // decode-ahead speaks to `AudioFile`/`audio_key()` directly, not
    // through `Spirc`/`Player`).
    let decode_ahead: SharedDecodeAhead = Arc::new(Mutex::new(None));
    // Set just before `spirc.activate()`/`spirc.transfer(None)`
    // (`RequestTransferHere`) and cleared the moment the resulting
    // `SessionConnected` is observed — tells the player-event task whether
    // that activation was this device's own request (`BecameActive
    // {context: None}`, US1's own flow / T18's "apply pending") or an
    // unsolicited transfer-to from another controller, which instead
    // waits up to 1 s for the following `TrackChanged` to build a
    // `TransferContext` (contracts/connect-source.md §3, research R3).
    let transfer_requested = Arc::new(AtomicBool::new(false));
    // Whether this device is the *active* Connect device. librespot ignores
    // `Load` (and most commands) while inactive, so a host-initiated
    // `LoadProgram` must `activate()` first — but only when we are not
    // already active, otherwise setting `transfer_requested` for an
    // `activate()` that raises no `SessionConnected` would leak onto a later
    // genuine remote transfer and suppress its context seeding (US3). Set by
    // the player-event task alongside every `BecameActive`/`BecameInactive`.
    let device_active = Arc::new(AtomicBool::new(false));
    // Catalog concurrency cap (004-search-and-library-browse, contracts/
    // catalog-source.md §3 "Threads"): further `SearchCatalog`/
    // `FetchLibrary`/`FetchTrackList`/`HydrateRefs` commands queue FIFO
    // behind this semaphore rather than each spawning unbounded tasks.
    // Created once per worker thread (outlives session reconnects, unlike
    // `session`/`spirc`).
    let catalog_semaphore = Arc::new(tokio::sync::Semaphore::new(crate::catalog::MAX_CONCURRENT));
    let mut backoff = Backoff::new();
    let mut sample_tx = Some(sample_tx);
    // Built exactly once (`RingSink`'s producer is move-only); subsequent
    // reconnects reuse it via `Player::set_session`.
    let mut player: Option<Arc<Player>> = None;

    loop {
        let token = match config.credentials.access_token() {
            Ok(token) => token,
            Err(_) => {
                let delay = backoff.next_delay();
                report_health(
                    &event_tx,
                    SourceHealth::Transient {
                        since: Instant::now(),
                        next_retry_in: delay,
                    },
                );
                let outcome =
                    wait_for_retry_or_shutdown(&cmd_rx, &event_tx, &config.tmp_dir, delay);
                if !resolve_wait(
                    outcome,
                    &cmd_rx,
                    &config.tmp_dir,
                    &mut pending_program,
                    &mut device_name,
                    &mut backoff,
                ) {
                    return;
                }
                continue;
            }
        };

        let session_config = build_session_config(&config);
        // `Session::new` captures `tokio::runtime::Handle::current()`
        // (librespot-core session.rs), so it must run inside this worker's
        // runtime context — otherwise it panics "there is no reactor
        // running". `Player::new` owns its own runtime and `set_session`
        // only sends a channel command, so only this call needs the guard.
        let session = {
            let _runtime_guard = runtime.enter();
            Session::new(session_config, None)
        };

        if player.is_none() {
            let Some(producer) = sample_tx.take() else {
                // The producer was already consumed by an earlier attempt
                // that failed before reaching this point — cannot build a
                // player without it; nothing more this worker can do.
                report_health(
                    &event_tx,
                    SourceHealth::Unavailable {
                        client_update_required: false,
                    },
                );
                return;
            };
            let player_config = PlayerConfig {
                gapless: true,
                position_update_interval: Some(Duration::from_millis(250)),
                ..PlayerConfig::default()
            };
            let written_frames_for_sink = Arc::clone(&written_frames);
            let swap_for_sink = Arc::clone(&swap);
            let shared_for_sink = Arc::clone(&shared);
            let sink_builder = move || -> Box<dyn librespot_playback::audio_backend::Sink> {
                Box::new(RingSink::new(
                    producer,
                    written_frames_for_sink,
                    swap_for_sink,
                    shared_for_sink,
                ))
            };
            player = Some(Player::new(
                player_config,
                session.clone(),
                Box::new(NoOpVolume),
                sink_builder,
            ));
        } else if let Some(player) = player.as_ref() {
            player.set_session(session.clone());
        }
        let Some(player) = player.clone() else {
            return;
        };

        let connect_config = SpircConnectConfig {
            name: device_name.clone(),
            device_type: DeviceType::Computer,
            is_group: false,
            initial_volume: crate::mixer::to_u16(mixer.volume_pct()),
            disable_volume: false,
            volume_steps: 64,
        };
        let credentials = Credentials::with_access_token(token);
        let mixer_dyn: Arc<dyn Mixer> = Arc::clone(&mixer) as Arc<dyn Mixer>;

        let spirc_result = runtime.block_on(Spirc::new(
            connect_config,
            session.clone(),
            credentials,
            Arc::clone(&player),
            mixer_dyn,
        ));

        let (spirc, spirc_task) = match spirc_result {
            Ok(parts) => parts,
            Err(error) => {
                match health::classify(error.kind) {
                    health::Classification::TierRejected => {
                        let _ = event_tx.send(SourceEvent::TierRejected);
                    }
                    health::Classification::Health(outcome) => {
                        let health =
                            health::to_source_health(outcome, Instant::now(), backoff.next_delay());
                        report_health(&event_tx, health);
                    }
                }
                let outcome = wait_for_retry_or_shutdown(
                    &cmd_rx,
                    &event_tx,
                    &config.tmp_dir,
                    backoff.next_delay(),
                );
                if !resolve_wait(
                    outcome,
                    &cmd_rx,
                    &config.tmp_dir,
                    &mut pending_program,
                    &mut device_name,
                    &mut backoff,
                ) {
                    return;
                }
                continue;
            }
        };

        backoff.reset();
        announce_session(&event_tx, &device_name);

        // T084 (research R6): whether the *current* track is inside
        // librespot's 30 s-before-end preload window, so a `LoadProgram`
        // arriving in that window knows to (re)prefetch the item that is
        // now next — reset per session, set by the player-event task on
        // `PlayerEvent::TimeToPreloadNextTrack`, cleared once the next
        // track actually starts.
        let preload_window_open = Arc::new(AtomicBool::new(false));

        // One `MapperState` per session, shared between the command loop
        // (which records each `LoadProgram` in it) and the player-event
        // task (which resolves `TrackChanged` against that program). The
        // 2026-09-17 manual walk (004 quickstart M4) found these had been
        // three independent `MapperState::default()`s, so the event task
        // never saw a program: every `TrackChanged` mapped to
        // `program: None` → `TrackRevealed`, and the host collapsed its
        // whole context to the single revealed track.
        let mapper: SharedMapper = Arc::new(Mutex::new(MapperState::default()));
        if let Some(program) = pending_program.take() {
            apply_load_program(&spirc, &mapper, &program, &player, &preload_window_open);
        }

        let ended = Arc::new(AtomicBool::new(false));
        // #31: this session's player-event task forwards only while the
        // session is live. `Player` (and its event channel) outlives the
        // session — across reconnects, and now across a parked
        // `Deregister` whose runtime stays up — so without this the old
        // task would keep forwarding: the teardown's own
        // `SessionDisconnected` surfaced as `BecameInactive` after
        // `Deregistered`, and every reconnect stacked another forwarder.
        let session_live = Arc::new(AtomicBool::new(true));
        {
            let ended = Arc::clone(&ended);
            runtime.spawn(async move {
                spirc_task.await;
                ended.store(true, Ordering::Release);
            });
        }

        let player_events = player.get_player_event_channel();
        let (marker_forward_tx, marker_forward_rx) = std::sync::mpsc::channel::<Marker>();
        {
            let event_tx = event_tx.clone();
            let mixer = Arc::clone(&mixer);
            let written_frames_for_task = Arc::clone(&written_frames);
            let transfer_requested = Arc::clone(&transfer_requested);
            let device_active = Arc::clone(&device_active);
            let preload_window_open = Arc::clone(&preload_window_open);
            let mapper = Arc::clone(&mapper);
            let decode_ahead = Arc::clone(&decode_ahead);
            let session_live = Arc::clone(&session_live);
            let session_for_decode = session.clone();
            let runtime_handle = runtime.handle().clone();
            runtime.spawn(async move {
                // 005-now-playing-waveform (contracts/connect-source-
                // delta.md §1-2): stop the previous track's decode-ahead
                // (its `Drop` does the stopping) and spawn a fresh one for
                // `audio_item`, before the `TrackStart` marker that
                // carries its store is pushed.
                let spawn_decode_ahead = |audio_item: &librespot_metadata::audio::AudioItem| {
                    let len_frames = u64::from(audio_item.duration_ms)
                        * u64::from(crate::rt::SAMPLE_RATE)
                        / 1000;
                    let store = DecodedStore::new(crate::rt::SAMPLE_RATE, len_frames);
                    let handle = decode_ahead::spawn(
                        session_for_decode.clone(),
                        audio_item.clone(),
                        Arc::clone(&store),
                        runtime_handle.clone(),
                        force_fail,
                    );
                    let mut guard = decode_ahead
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    *guard = Some(handle);
                    store
                };
                let mut player_events = player_events;
                // `Some(deadline)` while waiting to see whether an
                // unsolicited `SessionConnected` is a transfer-in with a
                // known context (contracts/connect-source.md §3).
                let mut awaiting_transfer_context: Option<Instant> = None;

                loop {
                    let event = match awaiting_transfer_context {
                        Some(deadline) => {
                            let remaining = deadline.saturating_duration_since(Instant::now());
                            match tokio::time::timeout(remaining, player_events.recv()).await {
                                Ok(Some(event)) => event,
                                Ok(None) => break,
                                Err(_) => {
                                    if !session_live.load(Ordering::Acquire) {
                                        break;
                                    }
                                    // No `TrackChanged` arrived in time:
                                    // this activation has no known context.
                                    awaiting_transfer_context = None;
                                    device_active.store(true, Ordering::Release);
                                    if event_tx
                                        .send(SourceEvent::BecameActive { context: None })
                                        .is_err()
                                    {
                                        break;
                                    }
                                    continue;
                                }
                            }
                        }
                        None => match player_events.recv().await {
                            Some(event) => event,
                            None => break,
                        },
                    };
                    if !session_live.load(Ordering::Acquire) {
                        break;
                    }

                    if matches!(event, PlayerEvent::SessionConnected { .. }) {
                        if transfer_requested.swap(false, Ordering::AcqRel) {
                            // This device asked for the transfer
                            // (`RequestTransferHere`/`activate`); T18's
                            // "apply pending" path needs no context.
                            device_active.store(true, Ordering::Release);
                            if event_tx
                                .send(SourceEvent::BecameActive { context: None })
                                .is_err()
                            {
                                break;
                            }
                        } else {
                            awaiting_transfer_context =
                                Some(Instant::now() + TRANSFER_CONTEXT_WINDOW);
                        }
                        continue;
                    }

                    if awaiting_transfer_context.is_some()
                        && let PlayerEvent::TrackChanged { audio_item } = &event
                    {
                        awaiting_transfer_context = None;
                        // Still stamp the real-time marker so the anchor
                        // resets for the newly-transferred track, even
                        // though the host-visible event below is
                        // `BecameActive`, not a plain `TrackStarted`
                        // (avoiding a spurious `Queue::reveal` for the
                        // same track `TransferContext::current` already
                        // names). The store is created — and the
                        // decode-ahead spawned — before this marker, per
                        // contracts/connect-source-delta.md §1's ordering.
                        let store = spawn_decode_ahead(audio_item);
                        let written = written_frames_for_task.load(Ordering::Acquire);
                        let _ = marker_forward_tx
                            .send(Marker::track_start(written, Arc::clone(&store)));
                        let context =
                            events::track_ref_from_audio_item(audio_item).map(|current| {
                                modplayer_audio_source::TransferContext {
                                    current,
                                    position_ms: 0,
                                    playing: true,
                                    shuffle: None,
                                    repeat: None,
                                }
                            });
                        let track_id = events::track_ref_from_audio_item(audio_item)
                            .map(|track_ref| track_ref.id);
                        device_active.store(true, Ordering::Release);
                        if event_tx
                            .send(SourceEvent::BecameActive { context })
                            .is_err()
                        {
                            break;
                        }
                        if let Some(track) = track_id {
                            let _ = event_tx.send(SourceEvent::DecodedStore { track, store });
                        }
                        continue;
                    }

                    // T084: the window in which a `LoadProgram` should
                    // (re)prefetch the next item — opens 30 s before the
                    // current track ends, closes the moment a new track
                    // actually starts (research R6).
                    match &event {
                        PlayerEvent::TimeToPreloadNextTrack { .. } => {
                            preload_window_open.store(true, Ordering::Release);
                        }
                        PlayerEvent::TrackChanged { .. } => {
                            preload_window_open.store(false, Ordering::Release);
                        }
                        _ => {}
                    }

                    // The store is created — and the decode-ahead spawned
                    // — before `events::map` builds the `TrackStart`
                    // marker that carries it (contracts/connect-source-
                    // delta.md §1's ordering).
                    let decoded_store = if let PlayerEvent::TrackChanged { audio_item } = &event {
                        Some((
                            events::track_ref_from_audio_item(audio_item).map(|t| t.id),
                            spawn_decode_ahead(audio_item),
                        ))
                    } else {
                        None
                    };

                    let written = written_frames_for_task.load(Ordering::Acquire);
                    let (source_event, marker) = {
                        let mut mapper = lock_mapper(&mapper);
                        events::map(
                            event,
                            &mut mapper,
                            &mixer,
                            written,
                            decoded_store.as_ref().map(|(_, store)| Arc::clone(store)),
                        )
                    };
                    if let Some(marker) = marker {
                        let _ = marker_forward_tx.send(marker);
                    }
                    if let Some(source_event) = source_event {
                        if matches!(source_event, SourceEvent::BecameInactive) {
                            device_active.store(false, Ordering::Release);
                        }
                        let is_track_started =
                            matches!(source_event, SourceEvent::TrackStarted { .. });
                        if event_tx.send(source_event).is_err() {
                            break;
                        }
                        if is_track_started && let Some((Some(track), store)) = decoded_store {
                            let _ = event_tx.send(SourceEvent::DecodedStore { track, store });
                        }
                    }
                }
            });
        }

        let outcome = command_loop(
            &spirc,
            &player,
            &mixer,
            &mapper,
            &cmd_rx,
            &mut marker_tx,
            &marker_forward_rx,
            &mut retired_rx,
            &swap,
            &decode_ahead,
            &ended,
            &transfer_requested,
            &device_active,
            &preload_window_open,
            runtime.handle(),
            &session,
            &event_tx,
            &catalog_semaphore,
        );
        session_live.store(false, Ordering::Release);
        drain_retired(&mut retired_rx);

        match outcome {
            SessionOutcome::Shutdown => {
                let _ = spirc.shutdown();
                crate::tmp::purge(&config.tmp_dir);
                return;
            }
            SessionOutcome::Deregistered => {
                let _ = spirc.disconnect(true);
                let _ = spirc.shutdown();
                let _ = event_tx.send(SourceEvent::Deregistered);
                // #31: park rather than exit — `Player` (and the ring
                // producer it owns) survives, so the next `Initialize`
                // (sign-in after sign-out/revoke, tier restored) can
                // re-register in-process instead of being a no-op against
                // a dead thread.
                match park_until_initialize(&cmd_rx, &config.tmp_dir, &mut pending_program) {
                    Parked::Resume { device_name: name } => {
                        device_name = name;
                        backoff.reset();
                    }
                    Parked::Exit => return,
                }
            }
            SessionOutcome::Retry => {
                let _ = spirc.shutdown();
                // loop back and reconnect immediately (backoff already reset above).
            }
            SessionOutcome::UnexpectedEnd => {
                let delay = backoff.next_delay();
                report_health(
                    &event_tx,
                    SourceHealth::Transient {
                        since: Instant::now(),
                        next_retry_in: delay,
                    },
                );
                let outcome =
                    wait_for_retry_or_shutdown(&cmd_rx, &event_tx, &config.tmp_dir, delay);
                if !resolve_wait(
                    outcome,
                    &cmd_rx,
                    &config.tmp_dir,
                    &mut pending_program,
                    &mut device_name,
                    &mut backoff,
                ) {
                    return;
                }
            }
            SessionOutcome::SetDeviceName(new_name) => {
                let _ = spirc.shutdown();
                device_name = new_name;
                // loop back and re-register under the new name immediately.
            }
        }
    }
}

/// A session was established: report recovery *before* registration
/// (#31). Every failure path reports `Health(Transient|Unavailable)`, so
/// without this `Ok` the host would mirror the last failure forever —
/// `is_online()` stays false (Library/Search "offline") and the 30 s
/// reconnect warning fires even though librespot authenticated.
fn announce_session(event_tx: &Sender<SourceEvent>, device_name: &str) {
    report_health(event_tx, SourceHealth::Ok);
    log::info!("connect worker: registered Connect device");
    let _ = event_tx.send(SourceEvent::Registered {
        device_name: device_name.to_string(),
    });
}

/// Send a `Health` event, logging it at `info` (#31: the transition must
/// be visible in the app log).
fn report_health(event_tx: &Sender<SourceEvent>, health: SourceHealth) {
    log::info!("connect worker: health {health:?}");
    let _ = event_tx.send(SourceEvent::Health(health));
}

/// How [`wait_for_retry_or_shutdown`] ended.
#[derive(Debug, PartialEq, Eq)]
enum WaitOutcome {
    /// Backoff elapsed (or `Retry` arrived): reconnect now.
    Retry,
    /// `Shutdown`, or the command channel closed: exit the worker.
    Exit,
    /// `Deregister` arrived: stop reconnecting and park (#31).
    Deregistered,
}

/// While disconnected (credential/connect failure), keep draining
/// commands so `Shutdown` is honoured promptly instead of only after a
/// full backoff sleep.
fn wait_for_retry_or_shutdown(
    cmd_rx: &Receiver<SourceCommand>,
    event_tx: &Sender<SourceEvent>,
    tmp_dir: &std::path::Path,
    delay: Duration,
) -> WaitOutcome {
    let deadline = Instant::now() + delay;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return WaitOutcome::Retry;
        }
        match cmd_rx.recv_timeout(remaining.min(Duration::from_millis(200))) {
            Ok(SourceCommand::Shutdown) => {
                crate::tmp::purge(tmp_dir);
                return WaitOutcome::Exit;
            }
            Ok(SourceCommand::Deregister) => {
                // #31: this used to report `Deregistered` and keep
                // retrying — re-registering a signed-out account on the
                // next successful connect.
                let _ = event_tx.send(SourceEvent::Deregistered);
                return WaitOutcome::Deregistered;
            }
            Ok(SourceCommand::Retry) => return WaitOutcome::Retry,
            Ok(_) => {}
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return WaitOutcome::Exit,
        }
    }
}

/// Act on a [`WaitOutcome`]; returns `false` when the worker should exit.
/// A `Deregistered` outcome parks until the next `Initialize`, adopting
/// its device name and restarting the backoff.
fn resolve_wait(
    outcome: WaitOutcome,
    cmd_rx: &Receiver<SourceCommand>,
    tmp_dir: &std::path::Path,
    pending_program: &mut Option<Program>,
    device_name: &mut String,
    backoff: &mut Backoff,
) -> bool {
    match outcome {
        WaitOutcome::Retry => true,
        WaitOutcome::Exit => false,
        WaitOutcome::Deregistered => {
            match park_until_initialize(cmd_rx, tmp_dir, pending_program) {
                Parked::Resume { device_name: name } => {
                    *device_name = name;
                    backoff.reset();
                    true
                }
                Parked::Exit => false,
            }
        }
    }
}

/// How [`park_until_initialize`] ended.
#[derive(Debug, PartialEq, Eq)]
enum Parked {
    /// `Initialize` arrived: re-register under `device_name`.
    Resume { device_name: String },
    /// `Shutdown`, or the command channel closed.
    Exit,
}

/// Deregistered (#31): hold the worker — and the `Player` whose ring
/// producer cannot be rebuilt — idle until the host sends `Initialize`
/// again (`ConnectSource::handle_initialize` forwards it to a live
/// worker). A `LoadProgram` sent meanwhile is kept for the next session,
/// like the one carried across a restart; everything else is moot while
/// no session exists.
fn park_until_initialize(
    cmd_rx: &Receiver<SourceCommand>,
    tmp_dir: &std::path::Path,
    pending_program: &mut Option<Program>,
) -> Parked {
    log::info!("connect worker: deregistered, parked until Initialize");
    loop {
        match cmd_rx.recv() {
            Ok(SourceCommand::Initialize { device_name, .. }) => {
                log::info!("connect worker: Initialize while parked, re-registering");
                return Parked::Resume { device_name };
            }
            Ok(SourceCommand::Shutdown) => {
                crate::tmp::purge(tmp_dir);
                return Parked::Exit;
            }
            Ok(SourceCommand::LoadProgram(program)) => *pending_program = Some(program),
            Ok(_) => {}
            Err(_) => return Parked::Exit,
        }
    }
}

enum SessionOutcome {
    Shutdown,
    Deregistered,
    Retry,
    UnexpectedEnd,
    SetDeviceName(String),
}

/// The connected-session command loop (contracts/connect-source.md §2).
/// Polls `cmd_rx` with a short timeout so it can also notice the spirc
/// task ending unexpectedly (`ended`).
#[allow(clippy::too_many_arguments)]
fn command_loop(
    spirc: &Spirc,
    player: &Arc<Player>,
    mixer: &Arc<HostMixer>,
    mapper: &SharedMapper,
    cmd_rx: &Receiver<SourceCommand>,
    marker_tx: &mut Producer<Marker>,
    marker_forward_rx: &Receiver<Marker>,
    retired_rx: &mut Consumer<Arc<DecodedStore>>,
    swap: &RingSwap,
    decode_ahead: &SharedDecodeAhead,
    ended: &AtomicBool,
    transfer_requested: &AtomicBool,
    device_active: &AtomicBool,
    preload_window_open: &AtomicBool,
    runtime: &tokio::runtime::Handle,
    session: &Session,
    event_tx: &Sender<SourceEvent>,
    catalog_semaphore: &Arc<tokio::sync::Semaphore>,
) -> SessionOutcome {
    loop {
        // A re-attached RT (`swap.rs`): adopt its marker and retirement
        // rings here, on the thread that owns both ends. The old marker
        // ring's consumer is gone with the old RT, so anything still
        // queued in it is dropped with it — the new RT already got its
        // own `Reattach` marker from `attach()`. The old retirement ring
        // is drained one last time first so no store is ever leaked.
        if let Some(fresh) = swap.take_marker() {
            *marker_tx = fresh;
        }
        if let Some(fresh) = swap.take_retired() {
            drain_retired(retired_rx);
            *retired_rx = fresh;
        }
        // Once per iteration (contracts/connect-source-delta.md §2), plus
        // immediately before every marker push below.
        drain_retired(retired_rx);

        // Forward any markers the event-mapper task produced since we
        // last looked (non-blocking: the RT ring is drained by the audio
        // callback, this is just relaying into it).
        while let Ok(marker) = marker_forward_rx.try_recv() {
            drain_retired(retired_rx);
            let _ = marker_tx.push(marker);
        }

        match cmd_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(SourceCommand::Shutdown) => {
                *decode_ahead
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
                return SessionOutcome::Shutdown;
            }
            Ok(SourceCommand::Deregister) => return SessionOutcome::Deregistered,
            Ok(SourceCommand::Retry) => return SessionOutcome::Retry,
            Ok(SourceCommand::SetDeviceName(name)) => return SessionOutcome::SetDeviceName(name),
            Ok(SourceCommand::LoadProgram(program)) => {
                // A freshly-registered Connect device is Not Active, and
                // librespot ignores `SpircCommand::Load` (and every command
                // bar Transfer/Activate) while inactive (connect spirc.rs:
                // "will be ignored while Not Active"). A host-initiated
                // `LoadProgram` — "Play from account", or any first local
                // play — must therefore activate this device first. Only
                // when actually inactive: `transfer_requested` marks the
                // resulting `BecameActive` as host-requested rather than a
                // remote transfer (contracts/connect-source.md §3), and
                // must not be armed for an `activate()` that (already
                // active) raises no `SessionConnected` to consume it.
                if !device_active.load(Ordering::Acquire) {
                    transfer_requested.store(true, Ordering::Release);
                    let _ = spirc.activate();
                }
                apply_load_program(spirc, mapper, &program, player, preload_window_open);
            }
            Ok(SourceCommand::Play) => {
                let _ = spirc.play();
            }
            Ok(SourceCommand::Pause) => {
                let _ = spirc.pause();
            }
            Ok(SourceCommand::Stop) => {
                let _ = spirc.pause();
                let _ = spirc.set_position_ms(0);
                // Contracts/connect-source-delta.md §2: `Stop` (like
                // `Shutdown`/the next `TrackChanged`) stops the current
                // decode-ahead — dropping it here runs its `Drop`.
                *decode_ahead
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
            }
            Ok(SourceCommand::Seek(ms)) => {
                let _ = spirc.set_position_ms(ms);
                // Forward the seek target as a frame hint to the current
                // decode-ahead (contracts/connect-source-delta.md §2),
                // checked by it between packets.
                if let Some(handle) = decode_ahead
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .as_ref()
                {
                    let frame = (u64::from(ms) * u64::from(crate::rt::SAMPLE_RATE)) / 1000;
                    handle.seek_hint(frame);
                }
            }
            Ok(SourceCommand::SkipNext) => {
                let _ = spirc.next();
            }
            Ok(SourceCommand::SkipPrev) => {
                let _ = spirc.prev();
            }
            Ok(SourceCommand::SetVolume(pct)) => {
                mixer.set_from_host(pct.value());
                let _ = spirc.set_volume(crate::mixer::to_u16(pct.value()));
            }
            Ok(SourceCommand::RequestTransferHere) => {
                // Simplification (US1 scope, contract §2 "open
                // verification 1"): always request activation rather than
                // distinguishing an already-active cluster.
                //
                // Set *before* the call so the player-event task's
                // `SessionConnected` handler (racing on another task) never
                // observes the flag still clear for a `SessionConnected`
                // this request itself caused (contracts/connect-source.md
                // §3: "not requested by the host").
                transfer_requested.store(true, Ordering::Release);
                let _ = spirc.activate();
            }
            // Catalog commands (004-search-and-library-browse, contracts/
            // catalog-source.md §1/§3): every one runs as a
            // `runtime.spawn`ed task behind `catalog_semaphore` — never on
            // this command loop, never on the RT thread (the retired
            // `ListAccountTracks` used the same pattern before T061 removed
            // it). `SearchCatalog`/`HydrateRefs` are fulfilled by
            // `catalog::search`/`catalog::hydrate` (US1, T036/T037);
            // `FetchLibrary`/`FetchTrackList` by `catalog::{collection,
            // playlists, track_lists}` (US2, T057-T059).
            Ok(SourceCommand::SearchCatalog {
                request_id,
                query,
                kinds,
                offset,
                limit,
            }) => {
                // `MODPLAYER_CATALOG_FORCE_429` (quickstart M12, US3 T072):
                // short-circuits every catalog command to the same
                // `RateLimited` reply the real endpoint would give, without
                // ever dispatching to the network.
                if crate::catalog::force_rate_limited_search(offset) {
                    let _ = event_tx.send(SourceEvent::SearchResult {
                        request_id,
                        result: Err(CatalogError::RateLimited {
                            retry_after_ms: None,
                        }),
                    });
                } else {
                    let semaphore = Arc::clone(catalog_semaphore);
                    let event_tx = event_tx.clone();
                    let session = session.clone();
                    runtime.spawn(async move {
                        let _permit = semaphore.acquire().await;
                        let result =
                            crate::catalog::search::search(&session, &query, &kinds, offset, limit)
                                .await;
                        let _ = event_tx.send(SourceEvent::SearchResult { request_id, result });
                    });
                }
            }
            Ok(SourceCommand::FetchLibrary {
                request_id,
                set,
                page,
                limit,
            }) => {
                if crate::catalog::force_rate_limited() {
                    let _ = event_tx.send(SourceEvent::LibraryPage {
                        request_id,
                        result: Err(CatalogError::RateLimited {
                            retry_after_ms: None,
                        }),
                    });
                } else {
                    let semaphore = Arc::clone(catalog_semaphore);
                    let event_tx = event_tx.clone();
                    let session = session.clone();
                    runtime.spawn(async move {
                        let _permit = semaphore.acquire().await;
                        let result = match set {
                            modplayer_audio_source::LibrarySet::Playlists => {
                                crate::catalog::playlists::fetch(&session, page, limit).await
                            }
                            _ => {
                                crate::catalog::collection::fetch(&session, set, page, limit).await
                            }
                        };
                        let _ = event_tx.send(SourceEvent::LibraryPage { request_id, result });
                    });
                }
            }
            Ok(SourceCommand::FetchTrackList { request_id, source }) => {
                if crate::catalog::force_rate_limited() {
                    let _ = event_tx.send(SourceEvent::TrackList {
                        request_id,
                        result: Err(CatalogError::RateLimited {
                            retry_after_ms: None,
                        }),
                    });
                } else {
                    let semaphore = Arc::clone(catalog_semaphore);
                    let event_tx = event_tx.clone();
                    let session = session.clone();
                    runtime.spawn(async move {
                        let _permit = semaphore.acquire().await;
                        let result = crate::catalog::track_lists::fetch(&session, &source).await;
                        let _ = event_tx.send(SourceEvent::TrackList { request_id, result });
                    });
                }
            }
            Ok(SourceCommand::HydrateRefs {
                request_id,
                tracks,
                albums,
                artists,
            }) => {
                // `HydrateRefs` has no `Result` reply (contracts/catalog-
                // source.md §2): forcing a rate limit here would have
                // nowhere to surface, so the override only touches the
                // four request/reply commands `worker.rs` and the UI
                // actually degrade on (Search/Library/detail/track-list).
                let semaphore = Arc::clone(catalog_semaphore);
                let event_tx = event_tx.clone();
                let session = session.clone();
                runtime.spawn(async move {
                    let _permit = semaphore.acquire().await;
                    let (tracks, albums, artists, missing) =
                        crate::catalog::hydrate::hydrate(&session, &tracks, &albums, &artists)
                            .await;
                    let _ = event_tx.send(SourceEvent::Hydrated {
                        request_id,
                        tracks,
                        albums,
                        artists,
                        missing,
                    });
                });
            }
            Ok(SourceCommand::CancelCatalog { .. }) => {
                // Best effort (contracts/catalog-source.md §1): no
                // in-flight cancellation tracking in this slice; a late
                // reply is still delivered and the host discards it by
                // `request_id`.
            }
            Ok(SourceCommand::ReportState { .. }) => {
                // FR-025/rule T24 ("every input that changes intent,
                // position, volume, repeat or track" reports state within
                // 1 s, coalesced <= 1/200 ms): a no-op here is the correct,
                // complete implementation. Every one of those changes
                // already reaches Spirc through this same command loop's
                // `Play`/`Pause`/`Seek`/`SetVolume`/`LoadProgram` arms (or,
                // for a purely host-side change with no Spirc call of its
                // own, has no observable state for another controller to
                // miss), and Spirc's own actor already republishes state
                // to the cluster on its `UPDATE_STATE_DELAY` (<= 200 ms)
                // — librespot exposes no separate "report now" call to
                // invoke a second time (contract §2).
            }
            Ok(SourceCommand::PrefetchHint { frame }) => {
                // 006-markers-loops-and-cues, contracts/engine-loop.md §1:
                // a loop region was armed — ask the current decode-ahead
                // to prioritise `frame` (the seam's incoming edge), never
                // touching `spirc`/`player` itself.
                decode_ahead::forward_prefetch_hint(decode_ahead, frame);
            }
            Ok(SourceCommand::Initialize { .. }) => {
                // Already connected; a repeated `Initialize` is a no-op.
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return SessionOutcome::Shutdown,
        }

        if ended.load(Ordering::Acquire) {
            return SessionOutcome::UnexpectedEnd;
        }
    }
}

fn apply_load_program(
    spirc: &Spirc,
    mapper: &SharedMapper,
    program: &Program,
    player: &Player,
    preload_window_open: &AtomicBool,
) {
    lock_mapper(mapper).set_program(ProgramMap::new(
        program.generation,
        program.order.clone(),
        program.cursor_index,
    ));
    let _ = spirc.repeat(program.repeat_all);
    let _ = spirc.repeat_track(program.repeat_one);
    let uris: Vec<String> = program
        .order
        .iter()
        .map(|id| id.as_str().to_string())
        .collect();
    let request = LoadRequest::from_tracks(
        uris,
        LoadRequestOptions {
            start_playing: program.start_playing,
            seek_to: program.position_ms,
            context_options: Some(LoadContextOptions::Options(Options {
                shuffle: false,
                repeat: program.repeat_all,
                repeat_track: program.repeat_one,
            })),
            playing_track: Some(PlayingTrack::Index(program.cursor_index)),
        },
    );
    let _ = spirc.load(request);

    // T084 (research R6): a program that lands while the current track is
    // inside the 30 s preload window must recompute/cancel a stale
    // prefetch for whatever is now next. `Player::preload` cancels
    // whatever it was preloading if the target differs, and is a no-op if
    // it is already preloading (or has already preloaded) the same track
    // (`handle_command_preload`), so no extra "did the next item change"
    // bookkeeping is needed here.
    if preload_window_open.load(Ordering::Acquire) {
        let next_index = if program.repeat_one {
            Some(program.cursor_index)
        } else if (program.cursor_index as usize + 1) < program.order.len() {
            Some(program.cursor_index + 1)
        } else if program.repeat_all && !program.order.is_empty() {
            Some(0)
        } else {
            None
        };
        if let Some(next_id) = next_index.and_then(|i| program.order.get(i as usize))
            && let Ok(uri) = SpotifyUri::from_uri(next_id.as_str())
        {
            player.preload(uri);
        }
    }
}

fn build_session_config(config: &WorkerConfig) -> SessionConfig {
    SessionConfig {
        device_id: config.device_id.clone(),
        tmp_dir: config.tmp_dir.clone(),
        ..SessionConfig::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn missing_tmp_dir() -> PathBuf {
        std::env::temp_dir().join("modplayer-worker-test-never-created")
    }

    fn program(generation: u64) -> Program {
        Program {
            order: Vec::new(),
            cursor_index: 0,
            position_ms: 0,
            start_playing: false,
            repeat_all: false,
            repeat_one: false,
            generation,
        }
    }

    fn initialize(name: &str) -> SourceCommand {
        SourceCommand::Initialize {
            device_name: name.to_string(),
            device_id: "0".repeat(32),
        }
    }

    #[test]
    fn announce_session_reports_health_ok_before_registered() {
        let (event_tx, event_rx) = std::sync::mpsc::channel();
        announce_session(&event_tx, "Den");
        let events: Vec<SourceEvent> = event_rx.try_iter().collect();
        assert!(matches!(
            events.as_slice(),
            [
                SourceEvent::Health(SourceHealth::Ok),
                SourceEvent::Registered { device_name },
            ] if device_name == "Den"
        ));
    }

    #[test]
    fn park_resumes_on_initialize_and_keeps_a_program_sent_meanwhile() {
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel();
        let _ = cmd_tx.send(SourceCommand::Play);
        let _ = cmd_tx.send(SourceCommand::LoadProgram(program(7)));
        let _ = cmd_tx.send(initialize("Kitchen"));
        let mut pending = None;
        let parked = park_until_initialize(&cmd_rx, &missing_tmp_dir(), &mut pending);
        assert_eq!(
            parked,
            Parked::Resume {
                device_name: "Kitchen".to_string()
            }
        );
        assert_eq!(pending.map(|p| p.generation), Some(7));
    }

    #[test]
    fn park_exits_on_shutdown() {
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel();
        let _ = cmd_tx.send(SourceCommand::Shutdown);
        let mut pending = None;
        assert_eq!(
            park_until_initialize(&cmd_rx, &missing_tmp_dir(), &mut pending),
            Parked::Exit
        );
    }

    #[test]
    fn park_exits_when_the_host_drops_the_channel() {
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<SourceCommand>();
        drop(cmd_tx);
        let mut pending = None;
        assert_eq!(
            park_until_initialize(&cmd_rx, &missing_tmp_dir(), &mut pending),
            Parked::Exit
        );
    }

    #[test]
    fn deregister_during_backoff_stops_retrying() {
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel();
        let (event_tx, event_rx) = std::sync::mpsc::channel();
        let _ = cmd_tx.send(SourceCommand::Deregister);
        let outcome = wait_for_retry_or_shutdown(
            &cmd_rx,
            &event_tx,
            &missing_tmp_dir(),
            Duration::from_secs(60),
        );
        assert_eq!(outcome, WaitOutcome::Deregistered);
        assert!(matches!(
            event_rx.try_iter().collect::<Vec<_>>().as_slice(),
            [SourceEvent::Deregistered]
        ));
    }

    #[test]
    fn resolve_wait_after_deregister_adopts_the_new_device_name() {
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel();
        let _ = cmd_tx.send(initialize("Office"));
        let mut pending = None;
        let mut device_name = "Old".to_string();
        let mut backoff = Backoff::new();
        let _ = backoff.next_delay();
        assert!(resolve_wait(
            WaitOutcome::Deregistered,
            &cmd_rx,
            &missing_tmp_dir(),
            &mut pending,
            &mut device_name,
            &mut backoff,
        ));
        assert_eq!(device_name, "Office");
    }
}

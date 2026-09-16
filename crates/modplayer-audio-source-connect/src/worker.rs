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
//! how librespot's own CLI handles session loss.
//!
//! Scope note (US1): a single automatic backoff-and-reconnect loop covers
//! an unexpected session end; the richer session-loss / transient-health
//! UI treatment (30 s warning, session-event handling) is US4's job
//! (T086-T090) — this worker already emits the `Health` events that later
//! phase reduces on.

use std::path::PathBuf;
use std::sync::Arc;
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
use modplayer_audio_source::{Program, SourceCommand, SourceEvent, SourceHealth, SourceRtShared};
use rtrb::Producer;

use crate::credentials::ReceiverCredentials;
use crate::events::{self, MapperState};
use crate::health::{self, Backoff};
use crate::mixer::HostMixer;
use crate::program::{Marker, ProgramMap};
use crate::sink::RingSink;

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
pub fn spawn(
    device_name: String,
    config: WorkerConfig,
    event_tx: Sender<SourceEvent>,
    sample_tx: Producer<f32>,
    marker_tx: Producer<Marker>,
    shared: Arc<SourceRtShared>,
    initial_program: Option<Program>,
) -> Option<WorkerHandles> {
    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<SourceCommand>();
    let builder = thread::Builder::new().name("connect-worker".to_string());
    let spawned = builder.spawn(move || {
        run(
            device_name,
            config,
            cmd_rx,
            event_tx,
            sample_tx,
            marker_tx,
            shared,
            initial_program,
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
    _shared: Arc<SourceRtShared>,
    mut pending_program: Option<Program>,
) {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => {
            let _ = event_tx.send(SourceEvent::Health(SourceHealth::Unavailable {
                client_update_required: false,
            }));
            return;
        }
    };

    let written_frames = Arc::new(AtomicU64::new(0));
    let mixer = Arc::new(HostMixer::new(config.initial_volume_pct));
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
                let _ = event_tx.send(SourceEvent::Health(SourceHealth::Transient {
                    since: Instant::now(),
                    next_retry_in: delay,
                }));
                if !wait_for_retry_or_shutdown(&cmd_rx, &event_tx, &config.tmp_dir, delay) {
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
                let _ = event_tx.send(SourceEvent::Health(SourceHealth::Unavailable {
                    client_update_required: false,
                }));
                return;
            };
            let player_config = PlayerConfig {
                gapless: true,
                position_update_interval: Some(Duration::from_millis(250)),
                ..PlayerConfig::default()
            };
            let written_frames_for_sink = Arc::clone(&written_frames);
            let sink_builder = move || -> Box<dyn librespot_playback::audio_backend::Sink> {
                Box::new(RingSink::new(producer, written_frames_for_sink))
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
                        let _ = event_tx.send(SourceEvent::Health(health));
                    }
                }
                if !wait_for_retry_or_shutdown(
                    &cmd_rx,
                    &event_tx,
                    &config.tmp_dir,
                    backoff.next_delay(),
                ) {
                    return;
                }
                continue;
            }
        };

        backoff.reset();
        let _ = event_tx.send(SourceEvent::Registered {
            device_name: device_name.clone(),
        });

        // T084 (research R6): whether the *current* track is inside
        // librespot's 30 s-before-end preload window, so a `LoadProgram`
        // arriving in that window knows to (re)prefetch the item that is
        // now next — reset per session, set by the player-event task on
        // `PlayerEvent::TimeToPreloadNextTrack`, cleared once the next
        // track actually starts.
        let preload_window_open = Arc::new(AtomicBool::new(false));

        let mut mapper = MapperState::default();
        if let Some(program) = pending_program.take() {
            apply_load_program(&spirc, &mut mapper, &program, &player, &preload_window_open);
        }

        let ended = Arc::new(AtomicBool::new(false));
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
            runtime.spawn(async move {
                let mut mapper = MapperState::default();
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
                        // names).
                        let written = written_frames_for_task.load(Ordering::Acquire);
                        let _ = marker_forward_tx.send(Marker::track_start(written));
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
                        device_active.store(true, Ordering::Release);
                        if event_tx
                            .send(SourceEvent::BecameActive { context })
                            .is_err()
                        {
                            break;
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

                    let written = written_frames_for_task.load(Ordering::Acquire);
                    let (source_event, marker) = events::map(event, &mut mapper, &mixer, written);
                    if let Some(marker) = marker {
                        let _ = marker_forward_tx.send(marker);
                    }
                    if let Some(source_event) = source_event {
                        if matches!(source_event, SourceEvent::BecameInactive) {
                            device_active.store(false, Ordering::Release);
                        }
                        if event_tx.send(source_event).is_err() {
                            break;
                        }
                    }
                }
            });
        }

        let outcome = command_loop(
            &spirc,
            &player,
            &mixer,
            &cmd_rx,
            &mut marker_tx,
            &marker_forward_rx,
            &ended,
            &transfer_requested,
            &device_active,
            &preload_window_open,
            runtime.handle(),
            &session,
            &event_tx,
        );

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
                return;
            }
            SessionOutcome::Retry => {
                let _ = spirc.shutdown();
                // loop back and reconnect immediately (backoff already reset above).
            }
            SessionOutcome::UnexpectedEnd => {
                let delay = backoff.next_delay();
                let _ = event_tx.send(SourceEvent::Health(SourceHealth::Transient {
                    since: Instant::now(),
                    next_retry_in: delay,
                }));
                if !wait_for_retry_or_shutdown(&cmd_rx, &event_tx, &config.tmp_dir, delay) {
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

/// While disconnected (credential/connect failure), keep draining
/// commands so `Shutdown` is honoured promptly instead of only after a
/// full backoff sleep; returns `false` when the worker should exit.
fn wait_for_retry_or_shutdown(
    cmd_rx: &Receiver<SourceCommand>,
    event_tx: &Sender<SourceEvent>,
    tmp_dir: &std::path::Path,
    delay: Duration,
) -> bool {
    let deadline = Instant::now() + delay;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return true;
        }
        match cmd_rx.recv_timeout(remaining.min(Duration::from_millis(200))) {
            Ok(SourceCommand::Shutdown) => {
                crate::tmp::purge(tmp_dir);
                return false;
            }
            Ok(SourceCommand::Deregister) => {
                let _ = event_tx.send(SourceEvent::Deregistered);
            }
            Ok(SourceCommand::Retry) => return true,
            Ok(_) => {}
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return false,
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
    cmd_rx: &Receiver<SourceCommand>,
    marker_tx: &mut Producer<Marker>,
    marker_forward_rx: &Receiver<Marker>,
    ended: &AtomicBool,
    transfer_requested: &AtomicBool,
    device_active: &AtomicBool,
    preload_window_open: &AtomicBool,
    runtime: &tokio::runtime::Handle,
    session: &Session,
    event_tx: &Sender<SourceEvent>,
) -> SessionOutcome {
    let mut mapper = MapperState::default();
    loop {
        // Forward any markers the event-mapper task produced since we
        // last looked (non-blocking: the RT ring is drained by the audio
        // callback, this is just relaying into it).
        while let Ok(marker) = marker_forward_rx.try_recv() {
            let _ = marker_tx.push(marker);
        }

        match cmd_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(SourceCommand::Shutdown) => return SessionOutcome::Shutdown,
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
                apply_load_program(spirc, &mut mapper, &program, player, preload_window_open);
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
            }
            Ok(SourceCommand::Seek(ms)) => {
                let _ = spirc.set_position_ms(ms);
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
            Ok(SourceCommand::ListAccountTracks { request_id, limit }) => {
                // "Play from account" over the session (spec Amendment
                // 2026-09-16): fetch off the command loop so a slow metadata
                // round-trip never stalls transport commands, and reply
                // through the same event channel the UI already drains.
                let session = session.clone();
                let event_tx = event_tx.clone();
                runtime.spawn(async move {
                    let result = crate::account_read::fetch_account_tracks(session, limit).await;
                    let _ = event_tx.send(SourceEvent::AccountTracks { request_id, result });
                });
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
    mapper: &mut MapperState,
    program: &Program,
    player: &Player,
    preload_window_open: &AtomicBool,
) {
    mapper.set_program(ProgramMap::new(program.generation, program.order.clone()));
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

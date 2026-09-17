// SPDX-License-Identifier: MIT OR Apache-2.0

//! `TransportState` and the pure `reduce` transport reducer
//! (data-model.md §3, contracts/transport-and-queue.md §2). This phase
//! implements rules T1-T12 (play/pause/stop/seek/skip/track-mirroring);
//! the transfer, health and session rules (T13-T24) land with the user
//! stories that need them (US3/US4) and extend `Input`/`reduce` in place.
//!
//! `reduce` never touches `Queue` or the engine/source directly — it is a
//! pure function from `(state, input)` to `(state, Vec<Effect>)`. Where a
//! rule needs a `Queue` mutation's *result* (e.g. skip-forward's
//! `Advance`), the caller (the controller) performs the mutation and feeds
//! the result back in as `Input::QueueChanged`.

use modplayer_audio_source::{
    RemoteCommand, Repeat, SourceCommand, SourceHealth, TrackId, TrackRef, TransferContext,
    VolumePercent,
};
use modplayer_engine::Command;

use crate::notifications::{
    KEY_STREAM_RECONNECT_WARNING, KEY_STREAM_SOURCE_UNAVAILABLE, KEY_STREAM_SOURCE_UPDATE_REQUIRED,
    KEY_SUBSCRIPTION_DOWNGRADED, KEY_TRANSFER_REQUEST_FAILED, NotificationAction, Severity,
};
use crate::queue::{AdvanceReason, PlaybackChange, QueueChange};

/// Playback intent (data-model.md §3.1), structurally mirroring
/// `modplayer_engine::Transport`; re-exported from `modplayer_audio_source`
/// so `SourceCommand::ReportState` (which cannot depend on this crate) and
/// this module name the same type.
pub use modplayer_audio_source::Intent;

/// Why a device is not currently the Connect-active one, or isn't
/// registered at all (data-model.md §3.1, §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NotRegisteredReason {
    /// Nothing has decided yet (e.g. before the first `set_playback_
    /// permitted` call).
    #[default]
    Unknown,
    PremiumRequired,
    SubscriptionNotVerified,
    SignedOut,
}

/// The command that triggered a transfer request, applied once
/// `BecameActive` arrives (data-model.md §3.2). Lands with US3 (T17/T18);
/// defined now so `ActiveState::TransferRequested` is nameable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingTransferCommand {
    Play,
    SkipForward,
    SkipBack,
    Seek(u32),
    PlayHere,
}

/// Connect device status (data-model.md §3.1, FR-016/018/019/027).
#[derive(Debug, Clone, PartialEq)]
pub enum ActiveState {
    NotRegistered {
        reason: NotRegisteredReason,
    },
    Inactive {
        other_device: Option<String>,
    },
    Active,
    TransferRequested {
        pending: Option<PendingTransferCommand>,
    },
}

impl Default for ActiveState {
    fn default() -> Self {
        ActiveState::NotRegistered {
            reason: NotRegisteredReason::default(),
        }
    }
}

/// `Stopped | Playing | Paused`, with `Buffering` as a `Playing` sub-state
/// (data-model.md §3.1). Position itself is not tracked here — the UI
/// reads `PositionClock` directly (engine-delta.md §3).
#[derive(Debug, Clone, PartialEq)]
pub struct TransportState {
    pub intent: Intent,
    /// Only meaningful while `intent == Playing` (research R5).
    pub buffering: bool,
    pub track_len_ms: Option<u32>,
    pub active: ActiveState,
    pub health: SourceHealth,
    /// The 30 s transient-health warning (T20, US4) has been raised and
    /// not yet cleared.
    pub reconnect_warning_raised: bool,
    /// The generation of the last `Program` the host sent
    /// (contracts/transport-and-queue.md §3); a `TrackStarted` whose
    /// generation is older is ignored (T12). `sync_program` itself (the
    /// thing that increments this by actually sending a program) lands
    /// with US2 (Phase 4).
    pub current_generation: u64,
    /// FR-027/rule T22 (US4): the account tier was rejected/downgraded
    /// while a track was playing — the current item is let to finish
    /// (`EndOfTrack` does not advance while this is set) before the
    /// device disables and deregisters.
    pub downgrade_pending: bool,
}

impl Default for TransportState {
    fn default() -> Self {
        Self {
            intent: Intent::Stopped,
            buffering: false,
            track_len_ms: None,
            active: ActiveState::default(),
            health: SourceHealth::Ok,
            reconnect_warning_raised: false,
            current_generation: 0,
            downgrade_pending: false,
        }
    }
}

/// A `Queue` mutation `reduce` asks the controller to perform
/// (contracts/transport-and-queue.md §2's `Queue(QueueOp)` effect). The
/// controller applies it to its `Queue` and feeds the resulting
/// `QueueChange` back in as `Input::QueueChanged`.
#[derive(Debug, Clone, PartialEq)]
pub enum QueueOp {
    Advance(AdvanceReason),
    SkipBack {
        position_ms: u32,
    },
    /// `program: None` on `TrackStarted` (T12): the source revealed a
    /// track the host didn't queue. The controller calls
    /// `Queue::reveal(track)` and then moves the cursor onto it.
    Reveal(TrackRef),
    /// `SourceEvent::Unavailable` (T14, FR-026): the controller applies
    /// `Queue::mark_unavailable` and raises the `queue-item-skipped-
    /// unavailable` notification (it needs the item's title, which this
    /// pure reducer doesn't have).
    MarkUnavailable(TrackId),
    /// `SourceEvent::BecameActive { context: Some(ctx) }` (T18, research
    /// R3): the controller calls `Queue::adopt_transfer_context(track)`
    /// (seeding a source-driven, single-item queue) and, when supplied,
    /// applies `shuffle`/`repeat` from the transfer context.
    AdoptTransferContext {
        track: TrackRef,
        shuffle: Option<bool>,
        repeat: Option<Repeat>,
    },
}

/// A pending timer request (T17/T19/T20, US3/US4). Defined now so
/// `Effect::Timer` is nameable; no rule in this phase emits one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerKind {
    Transfer,
    Reconnect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerCommand {
    Start(TimerKind),
    Cancel(TimerKind),
}

/// A side effect `reduce` asks the controller to perform
/// (contracts/transport-and-queue.md §2).
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    Engine(Command),
    Source(SourceCommand),
    /// Convert `position_ms` to source frames and seek both the engine and
    /// the source (T6/T8's `Engine(Seek)` + `Source(Seek)` pair) — kept
    /// out of `Command`/`SourceCommand` directly because the reducer has
    /// no sample-rate context. `position_frames` (005-now-playing-waveform,
    /// contracts/transport-delta.md §1), when `Some`, is the exact frame a
    /// waveform click/keyboard seek resolved to — the controller sends it
    /// to the engine as-is instead of re-deriving it from `position_ms`.
    SeekTo {
        position_ms: u32,
        position_frames: Option<u64>,
    },
    /// Ask the controller to build a `Program` from its `Queue`'s current
    /// effective order (at `position_ms`, with `start_playing`) and send
    /// it as `SourceCommand::LoadProgram` — the reducer stays pure and
    /// does not touch `Queue`/`Program` itself.
    LoadCurrentProgram {
        position_ms: u32,
        start_playing: bool,
    },
    Queue(QueueOp),
    /// `actions` is rendered as up to `MAX_NOTIFICATION_ACTIONS` buttons
    /// (empty for a plain informational notification, e.g. `stream-
    /// source-unavailable`'s **Open status page** + **Retry**, US4 T21).
    Notify {
        key: &'static str,
        severity: Severity,
        actions: Vec<NotificationAction>,
    },
    DismissNotify {
        key: &'static str,
    },
    Timer(TimerCommand),
    /// `BecameActive { context: None }` in `TransferRequested` (T18): the
    /// command that requested the transfer is applied now that this
    /// device is active. The controller runs it through its own
    /// `play`/`skip_forward`/`skip_back`/`seek` (a second, ordinary
    /// `reduce` pass) rather than the reducer synthesising engine/source
    /// effects it has no context (buffer readiness, sample rate) to build
    /// correctly.
    ApplyPendingTransferCommand(PendingTransferCommand),
    /// `RemoteCommand::Volume` (T15): update the shadow volume and the
    /// engine's gain, but — unlike `set_master_volume` — do not echo
    /// `SourceCommand::SetVolume` back to the source that just reported it.
    MirrorVolume(VolumePercent),
    /// `RemoteCommand::Shuffle` (T15): apply the mode and re-send the
    /// program, exactly like the user toggling shuffle locally.
    RemoteSetShuffle(bool),
    /// `RemoteCommand::Repeat` (T15): apply the mode and re-send the
    /// program, exactly like the user changing repeat locally.
    RemoteSetRepeat(Repeat),
}

/// Which command caused a `Queue` mutation, so `Input::QueueChanged` can
/// tell a user-initiated skip (which must re-send the program) apart from
/// the host-driven `EndOfTrack` mirror (which must not — the source
/// already advanced itself; T12's generation check is what re-syncs on a
/// mismatch).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueChangeOrigin {
    UserSkipForward,
    UserSkipBack,
    /// T7's seek-past-end `Advance` — a user action, so a `MoveTo` result
    /// reloads the program exactly like a skip.
    UserSeek,
    /// T14's `MarkUnavailable` mirror: the source did *not* advance on its
    /// own (it just reported the track unavailable), so a `MoveTo` result
    /// must reload the program exactly like a user-initiated skip.
    UnavailableMirror,
    EndOfTrackMirror,
}

/// An input to `reduce`: a user command, a mirrored source event, or a
/// `Queue` mutation's result fed back in (contracts/transport-and-queue.md
/// §2).
#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    Play {
        has_current: bool,
        track_loaded_in_source: bool,
        buffer_ready: bool,
    },
    Pause,
    Stop,
    Seek {
        position_ms: u32,
        /// The exact source-rate frame the seek target resolved to
        /// (005-now-playing-waveform, contracts/transport-delta.md §1):
        /// `Some` from `PlaybackController::seek_frames` (a waveform
        /// click/keyboard seek), `None` from `seek(Duration)`.
        position_frames: Option<u64>,
        buffer_ready: bool,
    },
    SkipForward,
    SkipBack {
        position_ms: u32,
    },
    QueueChanged {
        change: QueueChange,
        origin: QueueChangeOrigin,
    },
    /// `SourceEvent::TrackStarted { program: Some((generation, index)) }`
    /// (T12). `index` is not consulted by this phase's `reduce` (the
    /// controller mirrors the cursor itself); it is carried for the
    /// self-healing check later phases add.
    TrackStarted {
        generation: u64,
        position_ms: u32,
        playing: bool,
        track_len_ms: u32,
    },
    /// `SourceEvent::TrackStarted { program: None }` (T12).
    TrackRevealed {
        track: TrackRef,
    },
    /// Mirrored `SourceEvent::EndOfTrack` (T11): host-driven bookkeeping
    /// only, the source already continues on its own.
    EndOfTrack,
    /// `SourceEvent::Registered` (contracts/audio-source-host.md §3): a
    /// lightweight mirror so `ActiveState`/`disabled_reason()` are
    /// meaningful from US1 on. The full transfer-aware active/inactive
    /// distinction (T16-T19) is US3's job.
    Registered,
    /// `SourceEvent::Deregistered`.
    Deregistered,
    /// `SourceEvent::TierRejected` (FR-027): a simplified, immediate
    /// version of rule T22 — US4 (T088) refines this to let the current
    /// item finish first.
    TierRejected,
    /// `SourceEvent::Health` (contracts/audio-source-host.md §3): tracks
    /// the latest health value. The full reconnect-warning/notification
    /// behaviour (T20/T21) is US4's job; this phase only keeps
    /// `TransportState.health` current so `transport_enabled()`/
    /// `disabled_reason()` can react to it.
    Health(SourceHealth),
    /// `SourceEvent::Loading` (T13's buffering derivation, needed by US1
    /// for SC-002/FR-010): buffering starts while the source (re)loads
    /// audio.
    Loading,
    /// `SourceEvent::Playing` (T13): buffering clears once audio is
    /// flowing again.
    Playing,
    /// `SourceEvent::Unavailable` (T14, FR-026): the source refused
    /// `track`. The controller looks up its title (for the notification)
    /// before applying `QueueOp::MarkUnavailable`.
    Unavailable {
        track: TrackId,
    },
    /// `play_here()` (contracts/transport-and-queue.md §1, FR-018):
    /// explicitly ask the service to make this device active. Only
    /// meaningful while `Inactive` (the gate at the top of `reduce`); a
    /// no-op otherwise.
    PlayHere,
    /// `SourceEvent::BecameInactive` (T16): another device took over.
    BecameInactive,
    /// `SourceEvent::BecameActive { context }` (T18): this device is (now)
    /// the Connect-active one — either because it requested the transfer
    /// (`context: None`, apply the pending command) or because another
    /// controller transferred playback to it (`context: Some(..)`).
    BecameActive {
        context: Option<TransferContext>,
    },
    /// The 5 s transfer timer expired while `TransferRequested` (T19).
    TransferTimedOut,
    /// The 30 s reconnect-warning timer expired while `Health::Transient`
    /// (T20, US4).
    ReconnectTimedOut,
    /// `SourceEvent::RemoteCommand` (contracts/audio-source-host.md §3):
    /// already applied by the source where it can (play/pause/seek/skip);
    /// the host mirrors its own shadow state and, for shuffle/repeat,
    /// re-applies its own mode.
    RemoteCommand(RemoteCommand),
}

/// The pure transport reducer (contracts/transport-and-queue.md §2,
/// rules T1-T12 in this phase).
pub fn reduce(mut state: TransportState, input: Input) -> (TransportState, Vec<Effect>) {
    let mut effects = Vec::new();

    // T17/T19: while this device is not the Connect-active one, a
    // playback command either requests a transfer (play/skip/seek/
    // play_here) or is ignored (pause/stop) instead of running its normal
    // T1-T10 rule; the same commands are ignored outright (not
    // re-requested) while a transfer is already in flight.
    match &state.active {
        ActiveState::Inactive { .. } => {
            if let Some(pending) = pending_command_for(&input) {
                state.active = ActiveState::TransferRequested {
                    pending: Some(pending),
                };
                effects.push(Effect::Source(SourceCommand::RequestTransferHere));
                effects.push(Effect::Timer(TimerCommand::Start(TimerKind::Transfer)));
                return (state, effects);
            }
            if matches!(input, Input::Pause | Input::Stop) {
                return (state, effects);
            }
        }
        ActiveState::TransferRequested { .. }
            if pending_command_for(&input).is_some()
                || matches!(input, Input::Pause | Input::Stop) =>
        {
            return (state, effects);
        }
        _ => {}
    }

    match input {
        Input::Play {
            has_current,
            track_loaded_in_source,
            buffer_ready,
        } => {
            if !has_current {
                return (state, effects);
            }
            match state.intent {
                // T1
                Intent::Stopped => {
                    state.intent = Intent::Playing;
                    if track_loaded_in_source {
                        effects.push(Effect::Source(SourceCommand::Play));
                    } else {
                        effects.push(Effect::LoadCurrentProgram {
                            position_ms: 0,
                            start_playing: true,
                        });
                    }
                    effects.push(Effect::Engine(Command::Play));
                    state.buffering = !buffer_ready;
                }
                // T2
                Intent::Paused => {
                    state.intent = Intent::Playing;
                    effects.push(Effect::Source(SourceCommand::Play));
                    effects.push(Effect::Engine(Command::Play));
                    state.buffering = !buffer_ready;
                }
                // T3
                Intent::Playing => {}
            }
        }
        // T4
        Input::Pause => {
            if state.intent == Intent::Playing {
                state.intent = Intent::Paused;
                state.buffering = false;
                effects.push(Effect::Source(SourceCommand::Pause));
                effects.push(Effect::Engine(Command::Pause));
            }
        }
        // T5
        Input::Stop => {
            state.intent = Intent::Stopped;
            state.buffering = false;
            effects.push(Effect::Engine(Command::Stop));
            effects.push(Effect::Source(SourceCommand::Stop));
        }
        // T6/T7/T8
        Input::Seek {
            position_ms,
            position_frames,
            buffer_ready,
        } => {
            let past_end = state.track_len_ms.is_some_and(|len| position_ms >= len);
            if past_end {
                // T7: clamp handled by the queue's own advance (repeat-one
                // restarts at 0 via the `Restart` branch of
                // `QueueChanged`; otherwise the next item loads at 0). This
                // also clamps `position_frames` — a frame past the ms clamp
                // never reaches `SeekTo`.
                effects.push(Effect::Queue(QueueOp::Advance(AdvanceReason::SeekPastEnd)));
            } else if state.intent == Intent::Stopped {
                // T6
                state.intent = Intent::Paused;
                state.buffering = false;
                effects.push(Effect::SeekTo {
                    position_ms,
                    position_frames,
                });
            } else {
                // T8
                effects.push(Effect::SeekTo {
                    position_ms,
                    position_frames,
                });
                if state.intent == Intent::Playing {
                    state.buffering = !buffer_ready;
                }
            }
        }
        // T9
        Input::SkipForward => {
            effects.push(Effect::Queue(QueueOp::Advance(AdvanceReason::Skip)));
        }
        // T10
        Input::SkipBack { position_ms } => {
            effects.push(Effect::Queue(QueueOp::SkipBack { position_ms }));
        }
        // T9/T10/T11's queue-result mirroring.
        Input::QueueChanged { change, origin } => match change.playback {
            PlaybackChange::MoveTo(_) => {
                let user_initiated = matches!(
                    origin,
                    QueueChangeOrigin::UserSkipForward
                        | QueueChangeOrigin::UserSkipBack
                        | QueueChangeOrigin::UserSeek
                        | QueueChangeOrigin::UnavailableMirror
                );
                if user_initiated {
                    effects.push(Effect::LoadCurrentProgram {
                        position_ms: 0,
                        start_playing: state.intent == Intent::Playing,
                    });
                }
                // `EndOfTrackMirror`: the source already advanced itself;
                // T12's generation check re-syncs on a mismatch.
            }
            PlaybackChange::Restart => {
                effects.push(Effect::SeekTo {
                    position_ms: 0,
                    position_frames: None,
                });
            }
            PlaybackChange::EndOfQueue => {
                state.intent = Intent::Stopped;
                state.buffering = false;
                effects.push(Effect::Engine(Command::Stop));
                effects.push(Effect::Source(SourceCommand::Stop));
            }
            PlaybackChange::Empty => {
                state.intent = Intent::Stopped;
                state.buffering = false;
            }
            // FR-026's notification/skip bookkeeping lands with US4 (T14).
            PlaybackChange::Skipped(_) | PlaybackChange::None => {}
        },
        // T12
        Input::TrackStarted {
            generation,
            track_len_ms,
            ..
        } => {
            if generation == state.current_generation {
                state.track_len_ms = Some(track_len_ms);
            }
            // Older generations are stale and ignored, per contract.
        }
        Input::TrackRevealed { track } => {
            effects.push(Effect::Queue(QueueOp::Reveal(track)));
        }
        // T11 / T22: the natural end of the current track normally mirrors
        // the source's own advance, but while a downgrade is pending
        // (T22) it instead finishes the downgrade — the current item is
        // deliberately not replaced by the next one.
        Input::EndOfTrack => {
            if state.downgrade_pending {
                effects.extend(finish_downgrade(&mut state));
            } else {
                effects.push(Effect::Queue(QueueOp::Advance(AdvanceReason::TrackEnd)));
            }
        }
        Input::Registered => {
            if matches!(state.active, ActiveState::NotRegistered { .. }) {
                state.active = ActiveState::Active;
            }
        }
        // FR-027/T23 (US4): a `Deregister` the controller sent for a known
        // reason (sign-out, downgrade, tier rejection) already recorded
        // that reason directly on `active` before this mirror ever lands
        // — preserve it rather than clobbering it with `Unknown`, which is
        // reserved for an unexpected deregistration the controller didn't
        // already explain (e.g. while previously `Active`/`Inactive`).
        Input::Deregistered => {
            let reason = match state.active {
                ActiveState::NotRegistered { reason } => reason,
                _ => NotRegisteredReason::Unknown,
            };
            state.active = ActiveState::NotRegistered { reason };
        }
        // T22 (US4, FR-027): let the current track finish before disabling
        // — `Input::EndOfTrack` below checks `downgrade_pending` and
        // finishes the downgrade instead of advancing. Nothing is
        // currently playing (`Paused`/`Stopped`, or no current item at
        // all) → finish immediately, there is nothing to wait for.
        Input::TierRejected => {
            state.downgrade_pending = true;
            if state.intent != Intent::Playing {
                effects.extend(finish_downgrade(&mut state));
            }
        }
        // T20/T21 (US4): a health change may (a) start/cancel the 30 s
        // reconnect-warning timer and clear an already-raised warning once
        // healthy again, and (b) disable transport with a critical
        // notification on an unrecoverable failure, clearing it on
        // recovery. Duration alone never escalates `Transient` to
        // `Unavailable` (SC-009) — only a fresh classification from the
        // source does that, which this reducer just mirrors.
        Input::Health(health) => {
            let was_transient = matches!(state.health, SourceHealth::Transient { .. });
            let is_transient = matches!(health, SourceHealth::Transient { .. });
            let was_unavailable = matches!(state.health, SourceHealth::Unavailable { .. });
            let is_unavailable = matches!(health, SourceHealth::Unavailable { .. });

            if is_transient && !was_transient {
                effects.push(Effect::Timer(TimerCommand::Start(TimerKind::Reconnect)));
            } else if !is_transient && was_transient {
                effects.push(Effect::Timer(TimerCommand::Cancel(TimerKind::Reconnect)));
            }
            if !is_transient && state.reconnect_warning_raised {
                state.reconnect_warning_raised = false;
                effects.push(Effect::DismissNotify {
                    key: KEY_STREAM_RECONNECT_WARNING,
                });
            }

            if is_unavailable && !was_unavailable {
                let client_update_required = matches!(
                    health,
                    SourceHealth::Unavailable {
                        client_update_required: true
                    }
                );
                let (key, actions) = if client_update_required {
                    (
                        KEY_STREAM_SOURCE_UPDATE_REQUIRED,
                        vec![NotificationAction::OpenStatusPage],
                    )
                } else {
                    (
                        KEY_STREAM_SOURCE_UNAVAILABLE,
                        vec![
                            NotificationAction::OpenStatusPage,
                            NotificationAction::RetrySource,
                        ],
                    )
                };
                effects.push(Effect::Notify {
                    key,
                    severity: Severity::Critical,
                    actions,
                });
            } else if !is_unavailable && was_unavailable {
                effects.push(Effect::DismissNotify {
                    key: KEY_STREAM_SOURCE_UNAVAILABLE,
                });
                effects.push(Effect::DismissNotify {
                    key: KEY_STREAM_SOURCE_UPDATE_REQUIRED,
                });
            }

            state.health = health;
        }
        // T13 (buffering half only this phase; the 30 s warning/timer in
        // T20/T21 is US4's job).
        Input::Loading => {
            if state.intent == Intent::Playing {
                state.buffering = true;
            }
        }
        Input::Playing => {
            if state.intent == Intent::Playing {
                state.buffering = false;
            }
        }
        // T14
        Input::Unavailable { track } => {
            effects.push(Effect::Queue(QueueOp::MarkUnavailable(track)));
        }
        // T17: only meaningful while `Inactive` (the gate above); reaching
        // this arm means the device is already `Active`, `TransferRequested`
        // (also gated above), or `NotRegistered` — a no-op in every case.
        Input::PlayHere => {}
        // T16
        Input::BecameInactive => {
            state.intent = Intent::Paused;
            state.buffering = false;
            state.active = ActiveState::Inactive { other_device: None };
            effects.push(Effect::Engine(Command::Pause));
            // `Source(Deregister)` intentionally NOT sent — still
            // registered (contract). The other device's name is the
            // controller's job (research R3).
        }
        // T18
        Input::BecameActive { context } => match context {
            None => {
                let pending = match std::mem::replace(&mut state.active, ActiveState::Active) {
                    ActiveState::TransferRequested { pending } => pending,
                    _ => None,
                };
                if let Some(pending) = pending {
                    effects.push(Effect::ApplyPendingTransferCommand(pending));
                }
            }
            Some(ctx) => {
                state.active = ActiveState::Active;
                state.intent = if ctx.playing {
                    Intent::Playing
                } else {
                    Intent::Paused
                };
                state.buffering = false;
                state.track_len_ms = Some(ctx.current.duration_ms);
                effects.push(Effect::Queue(QueueOp::AdoptTransferContext {
                    track: ctx.current,
                    shuffle: ctx.shuffle,
                    repeat: ctx.repeat,
                }));
                effects.push(Effect::Engine(if ctx.playing {
                    Command::Play
                } else {
                    Command::Pause
                }));
            }
        },
        // T19
        Input::TransferTimedOut => {
            if matches!(state.active, ActiveState::TransferRequested { .. }) {
                state.active = ActiveState::Inactive { other_device: None };
                effects.push(Effect::Notify {
                    key: KEY_TRANSFER_REQUEST_FAILED,
                    severity: Severity::Warning,
                    actions: Vec::new(),
                });
            }
        }
        // T20 (US4): the 30 s reconnect-warning timer expired while still
        // `Transient` — raise the warning exactly once (a further timer
        // firing after it was already raised, or after health recovered
        // in between, is a no-op).
        Input::ReconnectTimedOut => {
            if matches!(state.health, SourceHealth::Transient { .. })
                && !state.reconnect_warning_raised
            {
                state.reconnect_warning_raised = true;
                effects.push(Effect::Notify {
                    key: KEY_STREAM_RECONNECT_WARNING,
                    severity: Severity::Warning,
                    actions: Vec::new(),
                });
            }
        }
        // T15 (folded into this phase alongside T16-T19: the mirroring
        // rule needs `ActiveState`/queue plumbing this phase already
        // introduces, and contracts/transport-and-queue.md §7 pins it
        // under the same `controller_streaming.rs` coverage as transfer).
        Input::RemoteCommand(cmd) => match cmd {
            RemoteCommand::Play => {
                state.intent = Intent::Playing;
                state.buffering = false;
            }
            RemoteCommand::Pause => {
                state.intent = Intent::Paused;
                state.buffering = false;
            }
            // Already applied by the source; position mirrors through the
            // audio clock, the cursor through the following `TrackStarted`
            // (T12) — nothing further for the reducer to do.
            RemoteCommand::Seek(_) | RemoteCommand::SkipNext | RemoteCommand::SkipPrev => {}
            RemoteCommand::Volume(pct) => effects.push(Effect::MirrorVolume(pct)),
            RemoteCommand::Shuffle(on) => effects.push(Effect::RemoteSetShuffle(on)),
            RemoteCommand::Repeat(mode) => effects.push(Effect::RemoteSetRepeat(mode)),
        },
    }

    (state, effects)
}

/// T22 (US4, FR-027): stop, deregister, disable with `PremiumRequired`, and
/// raise the downgrade warning — shared by the "nothing was playing"
/// immediate path and the "current track just finished" path in
/// `Input::EndOfTrack`.
fn finish_downgrade(state: &mut TransportState) -> Vec<Effect> {
    state.downgrade_pending = false;
    state.intent = Intent::Stopped;
    state.buffering = false;
    state.active = ActiveState::NotRegistered {
        reason: NotRegisteredReason::PremiumRequired,
    };
    vec![
        Effect::Engine(Command::Stop),
        Effect::Source(SourceCommand::Deregister),
        Effect::Notify {
            key: KEY_SUBSCRIPTION_DOWNGRADED,
            severity: Severity::Warning,
            actions: vec![NotificationAction::OpenUpgradePage],
        },
    ]
}

/// The `PendingTransferCommand` a user input maps to, if any (T17): the
/// commands that, while `Inactive`, request a transfer instead of running
/// their normal rule.
fn pending_command_for(input: &Input) -> Option<PendingTransferCommand> {
    match input {
        Input::Play { .. } => Some(PendingTransferCommand::Play),
        Input::SkipForward => Some(PendingTransferCommand::SkipForward),
        Input::SkipBack { .. } => Some(PendingTransferCommand::SkipBack),
        Input::Seek { position_ms, .. } => Some(PendingTransferCommand::Seek(*position_ms)),
        Input::PlayHere => Some(PendingTransferCommand::PlayHere),
        _ => None,
    }
}

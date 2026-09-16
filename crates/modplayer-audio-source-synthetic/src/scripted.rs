// SPDX-License-Identifier: MIT OR Apache-2.0

//! `ScriptedHost` + `ScriptedRt`: a scripted `SourceHost` test double
//! (contracts/audio-source-host.md §5) used by `modplayer-core`'s
//! controller tests (Phase 3+) to drive every FR-related branch
//! deterministically: throttled loads, unavailable tracks, remote
//! commands, Connect transfer, and health changes, over deterministic
//! per-track fixture audio.
//!
//! Real-time note: unlike `ConnectRtSource`, `ScriptedRt` is a test double,
//! not shipped on a real device's audio callback, so it may take a brief
//! lock in `fill`/`seek` for the sake of a simple, obviously-correct
//! implementation.

use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};

use modplayer_audio_source::{
    AccountReadError, AudioSource, BufferStatus, Program, RemoteCommand, SourceCommand,
    SourceEvent, SourceHealth, SourceHost, SourceRtShared, TrackId, TrackRef, TransferContext,
};

/// Deterministic fixture audio for the track at `order[track_index]`
/// (contracts/audio-source-host.md §5): a DC offset unique to the track
/// plus a low sine at a unique frequency, so tests can identify which
/// track is audible and detect gaps/clicks at a transition.
fn fixture_sample(track_index: usize, frame: u64, sample_rate: u32) -> f32 {
    let dc = 0.05 * (track_index as f32 + 1.0);
    let freq = 220.0 * (track_index as f32 + 1.0);
    let phase = (f64::from(freq) * frame as f64 / f64::from(sample_rate.max(1))).fract();
    dc + 0.1 * (phase as f32 * std::f32::consts::TAU).sin()
}

#[derive(Debug, Default)]
struct Shared {
    program: Option<Program>,
    /// How many `SourceCommand::LoadProgram` this host has received —
    /// exposed via `ScriptedHostHandle::load_count` so `modplayer-core`'s
    /// `sync_program` debounce/coalescing tests (US2 T068) can assert how
    /// many programs actually reached the source without a dedicated fake.
    load_count: u32,
    unavailable: HashSet<TrackId>,
    /// `Some(ms)` while a scripted delay holds the current load in
    /// `Loading` until `release_throttle` is called (contracts/audio-
    /// source-host.md §5's `throttle(ms)`).
    throttled: bool,
    health: SourceHealth,
    events: VecDeque<SourceEvent>,
    registered: bool,
    device_name: String,
    /// Scripted reply to `SourceCommand::ListAccountTracks`; `None` (the
    /// default) replies `Err(Unavailable)`.
    account_tracks: Option<Result<Vec<TrackRef>, AccountReadError>>,
}

/// A cloneable handle onto a `ScriptedHost`'s script state, usable after
/// the host itself has been moved into a `PlaybackController` (tests hold
/// the handle, the controller owns the host). Every script directive
/// (`throttle`, `unavailable`, `remote`, `transfer_in`, `transfer_out`,
/// `health`) is defined here; `ScriptedHost` exposes the same methods by
/// delegation for the common case of scripting before attaching.
#[derive(Clone)]
pub struct ScriptedHostHandle {
    shared: Arc<Mutex<Shared>>,
}

impl ScriptedHostHandle {
    #[allow(clippy::unwrap_used)]
    fn lock(&self) -> std::sync::MutexGuard<'_, Shared> {
        self.shared.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Hold the next `LoadProgram`'s track in `Loading` rather than
    /// immediately emitting `Playing`, until `release_throttle` is called
    /// (simulates the "un-streamed track" readiness threshold, FR-010).
    /// `ms` is documentary only — this double is driven by explicit
    /// `release_throttle` calls, not a real clock.
    pub fn throttle(&self, _ms: u64) {
        self.lock().throttled = true;
    }

    /// Release a previously-set `throttle`, emitting `Playing` for the
    /// currently loaded track.
    pub fn release_throttle(&self) {
        let mut lock = self.lock();
        lock.throttled = false;
        if let Some(program) = lock.program.clone() {
            lock.events.push_back(SourceEvent::Playing {
                position_ms: program.position_ms,
            });
        }
    }

    /// Mark `id` as refused by the service: the next `LoadProgram` that
    /// lands on it emits `Unavailable` instead of playing (FR-026).
    pub fn unavailable(&self, id: TrackId) {
        self.lock().unavailable.insert(id);
    }

    /// Emit a `RemoteCommand` as if another controller sent it
    /// (contracts/audio-source-host.md §3).
    pub fn remote(&self, cmd: RemoteCommand) {
        self.lock()
            .events
            .push_back(SourceEvent::RemoteCommand(cmd));
    }

    /// Simulate a Connect transfer-to, with or without an upstream context
    /// (data-model.md §6).
    pub fn transfer_in(&self, context: Option<TransferContext>) {
        self.lock()
            .events
            .push_back(SourceEvent::BecameActive { context });
    }

    /// Simulate a Connect transfer-away.
    pub fn transfer_out(&self) {
        self.lock().events.push_back(SourceEvent::BecameInactive);
    }

    /// Script a health transition, emitted on the next `poll()`.
    pub fn health(&self, health: SourceHealth) {
        let mut lock = self.lock();
        lock.health = health.clone();
        lock.events.push_back(SourceEvent::Health(health));
    }

    /// Push an arbitrary `SourceEvent` directly onto the script queue, for
    /// scenarios the other directives don't script directly (US4): a
    /// mid-track `Loading`/`Playing` pair with no accompanying
    /// `LoadProgram` (an underrun and its recovery), or a natural
    /// `EndOfTrack` the reducer's downgrade rule (T22) needs to observe.
    pub fn emit(&self, event: SourceEvent) {
        self.lock().events.push_back(event);
    }

    /// How many `SourceCommand::LoadProgram` this host has received so far
    /// this test (US2 T068: `sync_program` debounce/coalescing tests count
    /// sends without needing to inspect the private event queue).
    pub fn load_count(&self) -> u32 {
        self.lock().load_count
    }

    /// The most recently loaded `Program`, if any (US2 T068).
    pub fn last_program(&self) -> Option<Program> {
        self.lock().program.clone()
    }

    /// Script the reply the next `SourceCommand::ListAccountTracks` returns
    /// (Stage 2 "Play from account" over the session). `None` was never set
    /// → the host replies `Err(Unavailable)`.
    pub fn account_tracks(&self, result: Result<Vec<TrackRef>, AccountReadError>) {
        self.lock().account_tracks = Some(result);
    }
}

/// A scripted `SourceHost` test double (contracts/audio-source-host.md
/// §5). Script directives queue events that `poll()` drains in order,
/// exactly like a real implementor. Call `handle()` before moving the host
/// into a controller to keep scripting it afterwards.
pub struct ScriptedHost {
    handle: ScriptedHostHandle,
    rt_shared: Arc<SourceRtShared>,
    sample_rate: u32,
}

impl ScriptedHost {
    pub fn new() -> Self {
        Self {
            handle: ScriptedHostHandle {
                shared: Arc::new(Mutex::new(Shared::default())),
            },
            rt_shared: Arc::new(SourceRtShared::new()),
            sample_rate: 44_100,
        }
    }

    /// A cloneable handle sharing this host's script state, usable after
    /// the host is moved into a `PlaybackController`.
    pub fn handle(&self) -> ScriptedHostHandle {
        self.handle.clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Shared> {
        self.handle.lock()
    }

    pub fn throttle(&self, ms: u64) {
        self.handle.throttle(ms);
    }

    pub fn release_throttle(&self) {
        self.handle.release_throttle();
    }

    pub fn unavailable(&self, id: TrackId) {
        self.handle.unavailable(id);
    }

    pub fn remote(&self, cmd: RemoteCommand) {
        self.handle.remote(cmd);
    }

    pub fn transfer_in(&self, context: Option<TransferContext>) {
        self.handle.transfer_in(context);
    }

    pub fn transfer_out(&self) {
        self.handle.transfer_out();
    }

    pub fn health(&self, health: SourceHealth) {
        self.handle.health(health);
    }

    /// See `ScriptedHostHandle::emit`.
    pub fn emit(&self, event: SourceEvent) {
        self.handle.emit(event);
    }

    /// See `ScriptedHostHandle::load_count`.
    pub fn load_count(&self) -> u32 {
        self.handle.load_count()
    }

    /// See `ScriptedHostHandle::last_program`.
    pub fn last_program(&self) -> Option<Program> {
        self.handle.last_program()
    }
}

impl Default for ScriptedHost {
    fn default() -> Self {
        Self::new()
    }
}

impl SourceHost for ScriptedHost {
    type Rt = ScriptedRt;

    fn attach(&mut self, position_frames: u64) -> Self::Rt {
        ScriptedRt {
            shared: Arc::clone(&self.handle.shared),
            sample_rate: self.sample_rate,
            position: position_frames,
        }
    }

    fn command(&mut self, cmd: SourceCommand) {
        let mut lock = self.lock();
        match cmd {
            SourceCommand::Initialize { device_name, .. } => {
                lock.registered = true;
                lock.device_name = device_name.clone();
                lock.events
                    .push_back(SourceEvent::Registered { device_name });
            }
            SourceCommand::Deregister | SourceCommand::Shutdown => {
                lock.registered = false;
                lock.events.push_back(SourceEvent::Deregistered);
            }
            SourceCommand::Retry => {
                lock.health = SourceHealth::Ok;
                lock.events.push_back(SourceEvent::Health(SourceHealth::Ok));
            }
            SourceCommand::SetDeviceName(name) => {
                lock.device_name = name.clone();
                lock.events
                    .push_back(SourceEvent::Registered { device_name: name });
            }
            SourceCommand::LoadProgram(program) => {
                let track_id = program.order.get(program.cursor_index as usize).cloned();
                lock.program = Some(program.clone());
                lock.load_count += 1;
                match track_id {
                    Some(id) if lock.unavailable.contains(&id) => {
                        lock.events
                            .push_back(SourceEvent::Unavailable { track: id });
                    }
                    Some(_) if lock.throttled => {
                        lock.events.push_back(SourceEvent::Loading {
                            position_ms: program.position_ms,
                        });
                    }
                    Some(_) => {
                        lock.events.push_back(SourceEvent::Playing {
                            position_ms: program.position_ms,
                        });
                    }
                    None => {}
                }
            }
            SourceCommand::Play => {
                let position_ms = lock.program.as_ref().map_or(0, |p| p.position_ms);
                lock.events.push_back(SourceEvent::Playing { position_ms });
            }
            SourceCommand::Pause => {
                let position_ms = lock.program.as_ref().map_or(0, |p| p.position_ms);
                lock.events.push_back(SourceEvent::Paused { position_ms });
            }
            SourceCommand::Stop => {
                lock.events.push_back(SourceEvent::Stopped);
            }
            SourceCommand::Seek(ms) => {
                lock.events
                    .push_back(SourceEvent::Seeked { position_ms: ms });
            }
            SourceCommand::SkipNext | SourceCommand::SkipPrev => {}
            SourceCommand::SetVolume(_) => {}
            SourceCommand::RequestTransferHere => {}
            SourceCommand::ReportState { .. } => {}
            SourceCommand::ListAccountTracks { request_id, .. } => {
                let result = lock
                    .account_tracks
                    .clone()
                    .unwrap_or(Err(AccountReadError::Unavailable));
                lock.events
                    .push_back(SourceEvent::AccountTracks { request_id, result });
            }
        }
    }

    fn poll(&mut self) -> Vec<SourceEvent> {
        self.lock().events.drain(..).collect()
    }

    fn buffer_status(&self) -> BufferStatus {
        let lock = self.lock();
        BufferStatus {
            ring_fill_frames: if lock.throttled { 0 } else { u32::MAX },
            ready: !lock.throttled,
            current_prefetched: !lock.throttled,
            next: None,
        }
    }

    fn health(&self) -> SourceHealth {
        self.lock().health.clone()
    }

    fn rt_shared(&self) -> Arc<SourceRtShared> {
        Arc::clone(&self.rt_shared)
    }
}

/// The real-time half handed out by `ScriptedHost::attach` (contracts/
/// audio-source-host.md §5): produces deterministic fixture audio for
/// whichever track the last `LoadProgram` selected.
pub struct ScriptedRt {
    shared: Arc<Mutex<Shared>>,
    sample_rate: u32,
    position: u64,
}

impl ScriptedRt {
    #[allow(clippy::unwrap_used)]
    fn track_index(&self) -> usize {
        self.shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .program
            .as_ref()
            .map_or(0, |p| p.cursor_index as usize)
    }
}

impl AudioSource for ScriptedRt {
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn len_frames(&self) -> Option<u64> {
        None
    }

    fn position(&self) -> u64 {
        self.position
    }

    fn seek(&mut self, frame: u64) {
        self.position = frame;
    }

    fn fill(&mut self, out: &mut [f32]) {
        let track_index = self.track_index();
        let frames = out.len() / 2;
        for i in 0..frames {
            let sample = fixture_sample(track_index, self.position, self.sample_rate);
            out[i * 2] = sample;
            out[i * 2 + 1] = sample;
            self.position += 1;
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;
    use modplayer_audio_source::{Availability, TrackRef};

    fn track(id: &str) -> TrackId {
        TrackId::new(format!("spotify:track:{id}")).unwrap()
    }

    fn program(order: Vec<TrackId>, cursor_index: u32) -> Program {
        Program {
            order,
            cursor_index,
            position_ms: 0,
            start_playing: true,
            repeat_all: false,
            repeat_one: false,
            generation: 1,
        }
    }

    #[test]
    fn load_program_emits_playing_when_not_throttled() {
        let mut host = ScriptedHost::new();
        host.command(SourceCommand::LoadProgram(program(vec![track("a")], 0)));
        let events = host.poll();
        assert!(matches!(events.as_slice(), [SourceEvent::Playing { .. }]));
    }

    #[test]
    fn throttle_holds_loading_until_released() {
        let mut host = ScriptedHost::new();
        host.throttle(2_000);
        host.command(SourceCommand::LoadProgram(program(vec![track("a")], 0)));
        let events = host.poll();
        assert!(matches!(events.as_slice(), [SourceEvent::Loading { .. }]));
        host.release_throttle();
        let events = host.poll();
        assert!(matches!(events.as_slice(), [SourceEvent::Playing { .. }]));
    }

    #[test]
    fn unavailable_track_emits_unavailable_instead_of_playing() {
        let mut host = ScriptedHost::new();
        let id = track("bad");
        host.unavailable(id.clone());
        host.command(SourceCommand::LoadProgram(program(vec![id.clone()], 0)));
        let events = host.poll();
        assert!(matches!(
            events.as_slice(),
            [SourceEvent::Unavailable { track }] if *track == id
        ));
    }

    #[test]
    fn different_tracks_produce_different_fixture_audio() {
        let mut host = ScriptedHost::new();
        host.command(SourceCommand::LoadProgram(program(
            vec![track("a"), track("b")],
            0,
        )));
        let _ = host.poll();
        let mut rt_a = host.attach(0);
        let mut buf_a = vec![0.0f32; 8];
        rt_a.fill(&mut buf_a);

        host.command(SourceCommand::LoadProgram(program(
            vec![track("a"), track("b")],
            1,
        )));
        let _ = host.poll();
        let mut rt_b = host.attach(0);
        let mut buf_b = vec![0.0f32; 8];
        rt_b.fill(&mut buf_b);

        assert_ne!(buf_a, buf_b, "different tracks must sound different");
    }

    #[test]
    fn remote_command_is_queued_verbatim() {
        let mut host = ScriptedHost::new();
        host.remote(RemoteCommand::Play);
        assert_eq!(
            host.poll(),
            vec![SourceEvent::RemoteCommand(RemoteCommand::Play)]
        );
    }

    #[test]
    fn transfer_in_and_out_emit_expected_events() {
        let mut host = ScriptedHost::new();
        let ctx = TransferContext {
            current: TrackRef::new(
                track("a"),
                "Title",
                vec!["Artist".to_string()],
                None,
                None,
                1000,
                Availability::Available,
            ),
            position_ms: 0,
            playing: true,
            shuffle: None,
            repeat: None,
        };
        host.transfer_in(Some(ctx.clone()));
        assert_eq!(
            host.poll(),
            vec![SourceEvent::BecameActive { context: Some(ctx) }]
        );
        host.transfer_out();
        assert_eq!(host.poll(), vec![SourceEvent::BecameInactive]);
    }
}

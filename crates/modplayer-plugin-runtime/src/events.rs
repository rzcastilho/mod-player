// SPDX-License-Identifier: MIT OR Apache-2.0

//! `RuntimeEvent`: what a plugin's scheduler thread reports back to core,
//! drained in `tick()` (data-model.md §2, contracts/gateway-and-
//! runtime.md RT3-RT5, RT12).

use std::sync::atomic::{AtomicU8, AtomicU32, AtomicU64, Ordering};

use log::Level;

/// Why a single handler invocation was aborted (RT3).
#[derive(Debug, Clone, PartialEq)]
pub enum AbortCause {
    /// The interrupt fired: the handler ran past its 4 ms budget.
    Deadline,
    /// The handler raised a Lua error (or a bad-argument/type error).
    Exception(String),
    /// RT11: track-state restore between `TrackChanged` dequeue and
    /// delivery overran its own 4 ms deadline; the event is still
    /// delivered, with an empty per-track scope.
    RestoreTimeout,
}

/// Why a plugin was suspended outright (RT4, RT5, RT12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuspendCause {
    /// The aggregate 1 s CPU share was exceeded and the last sample was
    /// itself a `Deadline` abort (a looping/hanging handler).
    Hang,
    /// The aggregate 1 s CPU share was exceeded by many short handlers.
    CpuShare,
    /// `Lua::set_memory_limit`'s cap was hit.
    Memory,
    /// `api.ready()` was never called within the 5 s grace period.
    DidNotStart,
}

/// One report from a plugin's scheduler thread to core (data-model.md
/// §2). Drained by `drain_plugin_runtime_events()` every `tick()`.
#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeEvent {
    /// `api.ready()` was called for the first time (RT5).
    Ready,
    /// A single handler invocation aborted; the plugin keeps running.
    HandlerAborted { cause: AbortCause },
    /// The plugin is being torn down for a fault (RT4/RT5/RT12); the
    /// thread has already run (or, for `DidNotStart`, skipped) its
    /// `unloading` handler and is about to exit.
    Suspended { cause: SuspendCause },
    /// A `log.*` call or an aborted-handler error, tagged for the
    /// plugin's console (research R19).
    Log { level: Level, message: String },
    /// The thread has dropped its Lua state and is about to return; core
    /// reaps the `JoinHandle` in `reap_plugin_threads()` (RT8, L8).
    Exited,
}

/// Core-written atomics a plugin thread reads every scheduler wake to
/// know the current transport intent, the engine's live sample rate, and
/// whether the current track has changed since its last position sample
/// (research R5; data-model.md §2). `PlaybackController` updates this
/// once per `tick()`; the plugin thread never blocks on it.
#[derive(Debug, Default)]
pub struct PlaybackSnapshot {
    /// `0` = Stopped, `1` = Playing, `2` = Paused (mirrors
    /// `modplayer_capability_gateway::event::PlayState`).
    intent: AtomicU8,
    source_rate: AtomicU32,
    /// Bumped on every track change so a plugin's position timers know to
    /// cancel (RT9) and so RT11 knows a fresh per-track scope must load.
    track_generation: AtomicU64,
    /// Bumped on every seek (host- or plugin-initiated) and loop wrap,
    /// independent of `track_generation` (US3 T095, contracts/
    /// plugin-api-v1.md §4 `position`: "once after a seek/loop-wrap while
    /// paused"): a plugin thread that observes a change resets its
    /// position "changed" edge without cancelling its position timers
    /// (unlike a track change, which does both).
    position_epoch: AtomicU64,
}

/// `PlaybackSnapshot::intent`'s three states, as a plain Rust enum for
/// callers (both directions convert through `as u8`/`from_u8`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaySnapshotState {
    Stopped,
    Playing,
    Paused,
}

impl PlaySnapshotState {
    const fn as_u8(self) -> u8 {
        match self {
            PlaySnapshotState::Stopped => 0,
            PlaySnapshotState::Playing => 1,
            PlaySnapshotState::Paused => 2,
        }
    }

    const fn from_u8(v: u8) -> Self {
        match v {
            1 => PlaySnapshotState::Playing,
            2 => PlaySnapshotState::Paused,
            _ => PlaySnapshotState::Stopped,
        }
    }
}

impl PlaybackSnapshot {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_intent(&self, state: PlaySnapshotState) {
        self.intent.store(state.as_u8(), Ordering::Release);
    }

    #[must_use]
    pub fn intent(&self) -> PlaySnapshotState {
        PlaySnapshotState::from_u8(self.intent.load(Ordering::Acquire))
    }

    pub fn set_source_rate(&self, rate: u32) {
        self.source_rate.store(rate, Ordering::Release);
    }

    #[must_use]
    pub fn source_rate(&self) -> u32 {
        self.source_rate.load(Ordering::Acquire)
    }

    /// Called by core on every track change (including to/from no track).
    pub fn bump_track_generation(&self) -> u64 {
        self.track_generation.fetch_add(1, Ordering::AcqRel) + 1
    }

    #[must_use]
    pub fn track_generation(&self) -> u64 {
        self.track_generation.load(Ordering::Acquire)
    }

    /// Called by core on every seek (host or plugin) and loop wrap.
    pub fn bump_position_epoch(&self) {
        self.position_epoch.fetch_add(1, Ordering::AcqRel);
    }

    #[must_use]
    pub fn position_epoch(&self) -> u64 {
        self.position_epoch.load(Ordering::Acquire)
    }
}

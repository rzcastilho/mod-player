// SPDX-License-Identifier: MIT OR Apache-2.0

//! `Queue`: the host-owned, pure, in-memory play queue (data-model.md §2,
//! contracts/transport-and-queue.md §4). Cursor-based, with a FIFO
//! play-next block, shuffle, repeat off/one/all, and a bounded history.
//! No I/O, no randomness of its own — callers inject an RNG
//! (`QueueRng`/`XorShiftRng` below) so tests stay deterministic.
//!
//! `context` holds *every* item ever added as `Context`/`Revealed`, minus
//! removals — including already-played ones — so `skip_back` can look
//! backward; `context_pos` is the index of the current/most-recently
//! context-sourced item, and drives "the next upcoming context item" once
//! `play_next` drains (data-model.md §2.2's `Cursor` collapsed into this
//! single index plus the separately-owned `current`, for a simpler,
//! equally observable implementation).

use std::collections::VecDeque;

use modplayer_audio_source::{Availability, Program, Repeat, TrackId, TrackRef};

/// Stable identity for a queue row, monotonically assigned — survives
/// reorders (data-model.md §2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct QueueItemId(u64);

impl QueueItemId {
    /// The raw counter value (009, R21): the plugin-facing gateway id is
    /// a narrower `u32` — callers at that boundary truncate this
    /// (queues never approach 2^32 rows in a session).
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Where a `QueueItem` came from (data-model.md §2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Context,
    PlayNext,
    /// Appended from a source-driven transfer context (research R3);
    /// displayed like `Context`.
    Revealed,
}

/// One row of the queue.
#[derive(Debug, Clone, PartialEq)]
pub struct QueueItem {
    pub uid: QueueItemId,
    pub track: TrackRef,
    pub origin: Origin,
    /// Set when the source reported `Unavailable`/`availability ==
    /// Unavailable`; skipped by advancement.
    pub unavailable: bool,
}

/// Research R3: whether the host or the source currently owns the play
/// order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueMode {
    HostDriven,
    SourceDriven,
}

/// Why `advance` was called (contracts/transport-and-queue.md §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdvanceReason {
    TrackEnd,
    Skip,
    Removed,
    SeekPastEnd,
}

/// What playback must do in response to a `Queue` mutation
/// (data-model.md §2.2, contracts/transport-and-queue.md §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackChange {
    None,
    /// Restart the current item from position 0 (e.g. `skip_back` beyond
    /// the 3 s threshold).
    Restart,
    MoveTo(QueueItemId),
    /// Ran out of upcoming items with `repeat == Off`.
    EndOfQueue,
    /// The queue became empty.
    Empty,
    /// `uid` was unavailable and was skipped over.
    Skipped(QueueItemId),
}

/// The result of every mutating `Queue` operation
/// (contracts/transport-and-queue.md §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueChange {
    pub order_changed: bool,
    pub cursor_changed: bool,
    pub playback: PlaybackChange,
}

impl QueueChange {
    const fn none() -> Self {
        Self {
            order_changed: false,
            cursor_changed: false,
            playback: PlaybackChange::None,
        }
    }
}

/// The bound on `Queue::history` (data-model.md §2.2).
pub const HISTORY_LIMIT: usize = 100;

/// A minimal RNG seam so `Queue::set_shuffle`/`advance`'s wrap re-shuffle
/// take their randomness from the caller (contracts/transport-and-queue.md
/// §4: "no `rand` dependency"; tests inject a fixed seed).
pub trait QueueRng {
    fn next_u64(&mut self) -> u64;
}

/// A 20-line xorshift64* PRNG, seeded from OS entropy in production
/// (`XorShiftRng::seeded`) or a fixed value in tests (`from_seed`).
#[derive(Debug, Clone)]
pub struct XorShiftRng(u64);

impl XorShiftRng {
    /// Seed from OS entropy (`getrandom`); the controller's production RNG.
    pub fn seeded() -> Self {
        let mut buf = [0u8; 8];
        // A failed read (vanishingly rare) leaves `buf` zeroed; `from_seed`
        // maps that to a fixed non-zero seed so the generator never
        // degenerates.
        let _ = getrandom::fill(&mut buf);
        Self::from_seed(u64::from_le_bytes(buf))
    }

    /// A deterministic seed, for tests. `0` is remapped (a zero seed makes
    /// xorshift stick at zero forever).
    pub fn from_seed(seed: u64) -> Self {
        Self(if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        })
    }
}

impl QueueRng for XorShiftRng {
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

/// Fisher-Yates shuffle using an injected `QueueRng`.
fn shuffle_in_place<T>(items: &mut [T], rng: &mut impl QueueRng) {
    for i in (1..items.len()).rev() {
        let j = (rng.next_u64() % (i as u64 + 1)) as usize;
        items.swap(i, j);
    }
}

/// The full effective play order handed to a `SourceHost::command`
/// (`SourceCommand::LoadProgram`) — the shape `Queue::program()` computes;
/// the controller fills in the transport-derived fields (position,
/// `start_playing`, `generation`) to build the full `Program`.
#[derive(Debug, Clone, PartialEq)]
pub struct QueueProgram {
    pub order: Vec<TrackId>,
    pub cursor_index: u32,
}

impl QueueProgram {
    /// Fold in the transport-derived fields to build the wire `Program`.
    pub fn into_program(
        self,
        position_ms: u32,
        start_playing: bool,
        repeat_all: bool,
        repeat_one: bool,
        generation: u64,
    ) -> Program {
        Program {
            order: self.order,
            cursor_index: self.cursor_index,
            position_ms,
            start_playing,
            repeat_all,
            repeat_one,
            generation,
        }
    }
}

/// The host-owned play queue (data-model.md §2.2).
#[derive(Debug, Clone)]
pub struct Queue {
    context: Vec<QueueItem>,
    play_next: VecDeque<QueueItem>,
    current: Option<QueueItem>,
    /// Index into `context` of the current/most-recently context-sourced
    /// item; `None` before any context item has ever played.
    context_pos: Option<usize>,
    /// `Some` while shuffle is on: the upcoming order of `context` items
    /// (by uid), not yet played this cycle.
    shuffle_order: Option<VecDeque<QueueItemId>>,
    repeat: Repeat,
    history: VecDeque<QueueItemId>,
    mode: QueueMode,
    next_uid: u64,
}

impl Default for Queue {
    fn default() -> Self {
        Self::new()
    }
}

impl Queue {
    pub fn new() -> Self {
        Self {
            context: Vec::new(),
            play_next: VecDeque::new(),
            current: None,
            context_pos: None,
            shuffle_order: None,
            repeat: Repeat::Off,
            history: VecDeque::new(),
            mode: QueueMode::HostDriven,
            next_uid: 0,
        }
    }

    fn alloc_uid(&mut self) -> QueueItemId {
        let id = QueueItemId(self.next_uid);
        self.next_uid += 1;
        id
    }

    // -- Read accessors --------------------------------------------------

    pub fn is_empty(&self) -> bool {
        self.current.is_none() && self.context.is_empty() && self.play_next.is_empty()
    }

    pub fn current(&self) -> Option<&QueueItem> {
        self.current.as_ref()
    }

    pub fn mode(&self) -> QueueMode {
        self.mode
    }

    pub fn repeat(&self) -> Repeat {
        self.repeat
    }

    pub fn shuffle_enabled(&self) -> bool {
        self.shuffle_order.is_some()
    }

    pub fn history(&self) -> impl Iterator<Item = QueueItemId> + '_ {
        self.history.iter().copied()
    }

    pub fn play_next_items(&self) -> impl Iterator<Item = &QueueItem> {
        self.play_next.iter()
    }

    /// Look up an item by `uid`, wherever it currently lives (`current`,
    /// `play_next`, or `context`) — used to notify by title without
    /// mutating (FR-026, contracts/transport-and-queue.md §3).
    pub fn item(&self, uid: QueueItemId) -> Option<&QueueItem> {
        if let Some(current) = &self.current
            && current.uid == uid
        {
            return Some(current);
        }
        self.play_next
            .iter()
            .find(|i| i.uid == uid)
            .or_else(|| self.context.iter().find(|i| i.uid == uid))
    }

    /// The title of the first item matching `track_id`, wherever it lives
    /// (`current`, `play_next`, or `context`) — used to notify about a
    /// track the source just reported unavailable, before the mutation
    /// that acts on it (FR-026).
    pub fn track_title(&self, track_id: &TrackId) -> Option<String> {
        if let Some(current) = &self.current
            && &current.track.id == track_id
        {
            return Some(current.track.title.clone());
        }
        self.play_next
            .iter()
            .chain(self.context.iter())
            .find(|item| &item.track.id == track_id)
            .map(|item| item.track.title.clone())
    }

    /// Every item in the effective order (current first, then the
    /// play-next block, then upcoming context — data-model.md §2.2).
    pub fn effective_order(&self) -> Vec<&QueueItem> {
        let mut order = Vec::new();
        if let Some(current) = &self.current {
            order.push(current);
        }
        order.extend(self.play_next.iter());
        order.extend(self.upcoming_context_items());
        order
    }

    fn upcoming_context_items(&self) -> Vec<&QueueItem> {
        if let Some(shuffle) = &self.shuffle_order {
            shuffle
                .iter()
                .filter_map(|uid| self.context.iter().find(|i| i.uid == *uid))
                .collect()
        } else {
            let start = self.context_pos.map_or(0, |p| p + 1);
            self.context.iter().skip(start).collect()
        }
    }

    fn find_context_index(&self, uid: QueueItemId) -> Option<usize> {
        self.context.iter().position(|i| i.uid == uid)
    }

    fn set_current(&mut self, item: QueueItem) {
        self.push_history(item.uid);
        self.current = Some(item);
    }

    fn push_history(&mut self, uid: QueueItemId) {
        self.history.push_back(uid);
        while self.history.len() > HISTORY_LIMIT {
            self.history.pop_front();
        }
    }

    /// Force `HostDriven` mode: a queue mutation made while `SourceDriven`
    /// switches ownership back to the host first, before the mutation
    /// itself is applied (contracts/transport-and-queue.md §3).
    pub fn ensure_host_driven(&mut self) {
        self.mode = QueueMode::HostDriven;
    }

    // -- Mutating operations ----------------------------------------------

    /// Replace the whole context; clears `play_next`/shuffle, keeps
    /// `repeat`; cursor -> first item; `mode = HostDriven`.
    pub fn replace_context(&mut self, tracks: Vec<TrackRef>) -> QueueChange {
        self.context.clear();
        self.play_next.clear();
        self.shuffle_order = None;
        self.context_pos = None;
        self.current = None;
        self.mode = QueueMode::HostDriven;

        for track in tracks {
            let uid = self.alloc_uid();
            self.context.push(QueueItem {
                uid,
                track,
                origin: Origin::Context,
                unavailable: false,
            });
        }

        if self.context.is_empty() {
            return QueueChange {
                order_changed: true,
                cursor_changed: true,
                playback: PlaybackChange::Empty,
            };
        }

        self.context_pos = Some(0);
        let first = self.context[0].clone();
        self.set_current(first);
        QueueChange {
            order_changed: true,
            cursor_changed: true,
            playback: PlaybackChange::Restart,
        }
    }

    /// Replace the whole context like `replace_context`, then start at
    /// `cursor` (clamped to the last valid index) instead of the first
    /// item — **Play now** on a track row picked from a list that is not
    /// itself the current context (contracts/library-and-search-core.md
    /// §2, FR-006): the acting list becomes the new context, playback
    /// starts at the clicked track. `Restart`, like `replace_context`.
    pub fn replace_context_at(&mut self, tracks: Vec<TrackRef>, cursor: usize) -> QueueChange {
        let change = self.replace_context(tracks);
        if self.context.is_empty() {
            return change;
        }
        let index = cursor.min(self.context.len() - 1);
        if index != 0 {
            self.context_pos = Some(index);
            let item = self.context[index].clone();
            self.current = Some(item.clone());
            // Replace the history entry `replace_context` just pushed for
            // index 0 with the item playback actually starts on.
            self.history.pop_back();
            self.push_history(item.uid);
        }
        QueueChange {
            order_changed: true,
            cursor_changed: true,
            playback: PlaybackChange::Restart,
        }
    }

    /// Append tracks to the context (FR-011).
    pub fn add_context(&mut self, tracks: Vec<TrackRef>) -> QueueChange {
        if tracks.is_empty() {
            return QueueChange::none();
        }
        // Nothing currently playing (whether or not `context` already
        // holds exhausted/past items) -> the first newly appended item
        // becomes current.
        let auto_start = self.current.is_none();
        let insertion_start = self.context.len();
        let mut new_uids = Vec::with_capacity(tracks.len());
        for track in tracks {
            let uid = self.alloc_uid();
            new_uids.push(uid);
            self.context.push(QueueItem {
                uid,
                track,
                origin: Origin::Context,
                unavailable: false,
            });
        }
        if let Some(shuffle) = &mut self.shuffle_order {
            shuffle.extend(new_uids);
        }

        if auto_start {
            self.context_pos = Some(insertion_start);
            let first = self.context[insertion_start].clone();
            self.set_current(first);
            return QueueChange {
                order_changed: true,
                cursor_changed: true,
                playback: PlaybackChange::Restart,
            };
        }

        QueueChange {
            order_changed: true,
            cursor_changed: false,
            playback: PlaybackChange::None,
        }
    }

    /// Seed a source-driven queue from a Connect transfer-in context
    /// (T74, research R3, data-model.md §6): clears everything and adopts
    /// `track` as the sole current item (`Origin::Revealed`, matching what
    /// `reveal()` produces), `mode = SourceDriven`. Further tracks the
    /// source reveals as playback continues arrive through `reveal()`,
    /// growing the queue exactly like a natural source-driven session.
    pub fn adopt_transfer_context(&mut self, track: TrackRef) -> QueueChange {
        self.context.clear();
        self.play_next.clear();
        self.shuffle_order = None;
        self.context_pos = None;
        self.current = None;
        self.mode = QueueMode::SourceDriven;

        let uid = self.alloc_uid();
        let item = QueueItem {
            uid,
            track,
            origin: Origin::Revealed,
            unavailable: false,
        };
        self.context.push(item.clone());
        self.context_pos = Some(0);
        self.set_current(item);

        // `MoveTo`, not `Restart`: the source already has this track
        // loaded/playing (that is what transfer-in means) — the reducer's
        // `QueueChanged` handling for a non-user-initiated `MoveTo` does
        // nothing further (contracts/transport-and-queue.md §2), unlike
        // `Restart`, which would re-seek the engine/source to 0 and
        // interrupt the transfer.
        QueueChange {
            order_changed: true,
            cursor_changed: true,
            playback: PlaybackChange::MoveTo(uid),
        }
    }

    /// Append a single source-revealed track (research R3); does not move
    /// the cursor — call `reveal_and_advance_to` (or `move_current_to`)
    /// once the source confirms it started (data-model.md §2.2).
    pub fn reveal(&mut self, track: TrackRef) -> QueueItemId {
        let uid = self.alloc_uid();
        self.context.push(QueueItem {
            uid,
            track,
            origin: Origin::Revealed,
            unavailable: false,
        });
        if let Some(shuffle) = &mut self.shuffle_order {
            shuffle.push_back(uid);
        }
        uid
    }

    /// Move the cursor to a specific context item without a `play_next`
    /// detour (T12's `Reveal` mirror, and general "jump to" for tests).
    pub fn move_current_to(&mut self, uid: QueueItemId) -> QueueChange {
        let Some(index) = self.find_context_index(uid) else {
            return QueueChange::none();
        };
        self.context_pos = Some(index);
        if let Some(shuffle) = &mut self.shuffle_order {
            shuffle.retain(|id| *id != uid);
        }
        let item = self.context[index].clone();
        self.set_current(item);
        QueueChange {
            order_changed: false,
            cursor_changed: true,
            playback: PlaybackChange::MoveTo(uid),
        }
    }

    /// Move an existing item (anywhere) to the tail of the play-next block
    /// (FR-011, FIFO).
    pub fn play_next(&mut self, uid: QueueItemId) -> QueueChange {
        if let Some(index) = self.play_next.iter().position(|i| i.uid == uid) {
            if let Some(item) = self.play_next.remove(index) {
                self.play_next.push_back(item);
            }
            return QueueChange {
                order_changed: true,
                cursor_changed: false,
                playback: PlaybackChange::None,
            };
        }
        if let Some(index) = self.find_context_index(uid) {
            let mut item = self.context.remove(index);
            item.origin = Origin::PlayNext;
            if let Some(pos) = &mut self.context_pos
                && index <= *pos
            {
                *pos = pos.saturating_sub(1);
            }
            if let Some(shuffle) = &mut self.shuffle_order {
                shuffle.retain(|id| *id != uid);
            }
            self.play_next.push_back(item);
            return QueueChange {
                order_changed: true,
                cursor_changed: false,
                playback: PlaybackChange::None,
            };
        }
        QueueChange::none()
    }

    /// Append a brand-new track directly to the tail of the play-next
    /// block.
    pub fn play_next_track(&mut self, track: TrackRef) -> QueueChange {
        let uid = self.alloc_uid();
        let auto_start = self.current.is_none();
        self.play_next.push_back(QueueItem {
            uid,
            track,
            origin: Origin::PlayNext,
            unavailable: false,
        });
        if auto_start {
            return self.advance_internal(AdvanceReason::Skip, &mut XorShiftRng::from_seed(1));
        }
        QueueChange {
            order_changed: true,
            cursor_changed: false,
            playback: PlaybackChange::None,
        }
    }

    /// Append several new tracks directly to the tail of the play-next
    /// block, preserving order — **Play next** on an album/playlist/artist
    /// row (contracts/library-and-search-core.md §2, FR-007): repeated
    /// `play_next_track`, so the first call's auto-start behaviour (queue
    /// was empty) still applies to the very first of `tracks`.
    pub fn play_next_tracks(&mut self, tracks: Vec<TrackRef>) -> QueueChange {
        let mut order_changed = false;
        let mut cursor_changed = false;
        let mut playback = PlaybackChange::None;
        for track in tracks {
            let change = self.play_next_track(track);
            order_changed |= change.order_changed;
            cursor_changed |= change.cursor_changed;
            if matches!(playback, PlaybackChange::None) {
                playback = change.playback;
            }
        }
        QueueChange {
            order_changed,
            cursor_changed,
            playback,
        }
    }

    /// Move `uid` one slot earlier in the effective order (current is
    /// index 0 and never moves). Crossing the play-next block's boundary
    /// updates the item's `origin` (FR-012). Manual reordering supersedes
    /// shuffle for the moved tail, so shuffle is turned off.
    pub fn move_up(&mut self, uid: QueueItemId) -> QueueChange {
        self.reorder_tail(uid, |pos| pos.checked_sub(1))
    }

    /// Move `uid` one slot later in the effective order.
    pub fn move_down(&mut self, uid: QueueItemId) -> QueueChange {
        self.reorder_tail(uid, |pos| Some(pos + 1))
    }

    /// Move `uid` to `to_effective_index` (0 = current; never moves the
    /// current item itself).
    pub fn reorder(&mut self, uid: QueueItemId, to_effective_index: usize) -> QueueChange {
        self.reorder_tail(uid, |_| Some(to_effective_index.saturating_sub(1)))
    }

    fn reorder_tail(
        &mut self,
        uid: QueueItemId,
        target: impl FnOnce(usize) -> Option<usize>,
    ) -> QueueChange {
        if self.current.as_ref().is_some_and(|c| c.uid == uid) {
            return QueueChange::none();
        }
        let mut tail: Vec<QueueItem> = self.play_next.iter().cloned().collect();
        tail.extend(self.upcoming_context_items().into_iter().cloned());
        let Some(from) = tail.iter().position(|i| i.uid == uid) else {
            return QueueChange::none();
        };
        let Some(to) = target(from) else {
            return QueueChange::none();
        };
        let to = to.min(tail.len().saturating_sub(1));
        if to == from {
            return QueueChange::none();
        }
        let item = tail.remove(from);
        tail.insert(to, item);

        self.apply_tail(tail);
        QueueChange {
            order_changed: true,
            cursor_changed: false,
            playback: PlaybackChange::None,
        }
    }

    /// Replace the whole upcoming (post-current) order with `tail`: the
    /// first `play_next.len()` items become the new play-next block
    /// (origin `PlayNext`), the rest become upcoming context (origin
    /// `Context`, appended after whatever already played). Turns shuffle
    /// off — an explicit manual order supersedes it.
    fn apply_tail(&mut self, tail: Vec<QueueItem>) {
        let split = self.play_next.len().min(tail.len());
        let (play_next_part, context_part) = tail.split_at(split);

        self.play_next = play_next_part
            .iter()
            .cloned()
            .map(|mut i| {
                i.origin = Origin::PlayNext;
                i
            })
            .collect();

        self.context.truncate(self.context_pos.map_or(0, |p| p + 1));
        for mut item in context_part.iter().cloned() {
            if item.origin == Origin::PlayNext {
                item.origin = Origin::Context;
            }
            self.context.push(item);
        }
        self.shuffle_order = None;
    }

    /// Remove `uid` from wherever it is. Removing the current item skips
    /// forward past it (ignoring repeat-one).
    pub fn remove(&mut self, uid: QueueItemId) -> QueueChange {
        if self.current.as_ref().is_some_and(|c| c.uid == uid) {
            return self.advance_internal(AdvanceReason::Removed, &mut XorShiftRng::from_seed(1));
        }
        if let Some(index) = self.play_next.iter().position(|i| i.uid == uid) {
            self.play_next.remove(index);
            return QueueChange {
                order_changed: true,
                cursor_changed: false,
                playback: PlaybackChange::None,
            };
        }
        if let Some(index) = self.find_context_index(uid) {
            self.context.remove(index);
            if let Some(pos) = &mut self.context_pos {
                if index < *pos {
                    *pos -= 1;
                } else if index == *pos {
                    // Removing an already-played context item at the
                    // cursor itself (not current — current is checked
                    // above): step the cursor back so the "next upcoming"
                    // computation stays correct.
                    *pos = pos.saturating_sub(1);
                }
            }
            if let Some(shuffle) = &mut self.shuffle_order {
                shuffle.retain(|id| *id != uid);
            }
            return QueueChange {
                order_changed: true,
                cursor_changed: false,
                playback: PlaybackChange::None,
            };
        }
        QueueChange::none()
    }

    /// Turn shuffle on/off (FR-013). Turning on permutes the remaining
    /// context after the cursor with `rng`, keeping `current` and
    /// `play_next` in place; turning off restores natural context order.
    pub fn set_shuffle(&mut self, on: bool, rng: &mut impl QueueRng) -> QueueChange {
        let was_on = self.shuffle_order.is_some();
        if on == was_on {
            return QueueChange::none();
        }
        self.shuffle_order = if on {
            Some(self.build_shuffle_order(rng))
        } else {
            None
        };
        QueueChange {
            order_changed: true,
            cursor_changed: false,
            playback: PlaybackChange::None,
        }
    }

    fn build_shuffle_order(&self, rng: &mut impl QueueRng) -> VecDeque<QueueItemId> {
        let start = self.context_pos.map_or(0, |p| p + 1);
        let mut ids: Vec<QueueItemId> = self.context[start..].iter().map(|i| i.uid).collect();
        shuffle_in_place(&mut ids, rng);
        ids.into()
    }

    pub fn set_repeat(&mut self, mode: Repeat) -> QueueChange {
        if self.repeat == mode {
            return QueueChange::none();
        }
        self.repeat = mode;
        QueueChange {
            order_changed: true,
            cursor_changed: false,
            playback: PlaybackChange::None,
        }
    }

    /// Advance the cursor (contracts/transport-and-queue.md §4). Skips
    /// items marked `unavailable` automatically so the returned change
    /// never names one as the new current item; `EndOfQueue`/`Empty` are
    /// the only "nothing left" outcomes.
    pub fn advance(&mut self, reason: AdvanceReason, rng: &mut impl QueueRng) -> QueueChange {
        self.advance_internal(reason, rng)
    }

    fn advance_internal(&mut self, reason: AdvanceReason, rng: &mut impl QueueRng) -> QueueChange {
        if self.repeat == Repeat::One
            && matches!(reason, AdvanceReason::TrackEnd | AdvanceReason::SeekPastEnd)
            && let Some(current) = self.current.clone()
        {
            self.push_history(current.uid);
            return QueueChange {
                order_changed: false,
                cursor_changed: false,
                playback: PlaybackChange::Restart,
            };
        }

        loop {
            if let Some(item) = self.play_next.pop_front() {
                if item.unavailable {
                    return QueueChange {
                        order_changed: true,
                        cursor_changed: false,
                        playback: PlaybackChange::Skipped(item.uid),
                    };
                }
                self.set_current(item.clone());
                return QueueChange {
                    order_changed: true,
                    cursor_changed: true,
                    playback: PlaybackChange::MoveTo(item.uid),
                };
            }

            if let Some(uid) = self.next_context_uid() {
                let index = self
                    .find_context_index(uid)
                    .unwrap_or(self.context_pos.map_or(0, |p| p + 1));
                self.context_pos = Some(index);
                if let Some(shuffle) = &mut self.shuffle_order {
                    shuffle.retain(|id| *id != uid);
                }
                let item = self.context[index].clone();
                if item.unavailable {
                    return QueueChange {
                        order_changed: false,
                        cursor_changed: true,
                        playback: PlaybackChange::Skipped(uid),
                    };
                }
                self.set_current(item.clone());
                return QueueChange {
                    order_changed: false,
                    cursor_changed: true,
                    playback: PlaybackChange::MoveTo(uid),
                };
            }

            if self.repeat == Repeat::All && !self.context.is_empty() {
                self.context_pos = None;
                if self.shuffle_order.is_some() {
                    self.shuffle_order = Some(self.build_shuffle_order(rng));
                }
                continue;
            }

            let had_current = self.current.is_some();
            self.current = None;
            return QueueChange {
                order_changed: false,
                cursor_changed: had_current,
                playback: if had_current {
                    PlaybackChange::EndOfQueue
                } else {
                    PlaybackChange::Empty
                },
            };
        }
    }

    fn next_context_uid(&self) -> Option<QueueItemId> {
        if let Some(shuffle) = &self.shuffle_order {
            shuffle.front().copied()
        } else {
            let next_index = self.context_pos.map_or(0, |p| p + 1);
            self.context.get(next_index).map(|i| i.uid)
        }
    }

    /// The item `advance(TrackEnd)` would choose next, without mutating
    /// (drives the "pre-fetch target recomputed within 1 s" rule, FR-009).
    /// Ignores repeat-one (that is a same-track restart, not a prefetch
    /// target) and the wrap re-shuffle (best-effort: reports the *next*
    /// context item pre-wrap-reshuffle when the upcoming list is empty).
    pub fn next_prefetch_target(&self) -> Option<TrackId> {
        if let Some(item) = self.play_next.front() {
            return Some(item.track.id.clone());
        }
        if let Some(uid) = self.next_context_uid() {
            return self
                .context
                .iter()
                .find(|i| i.uid == uid)
                .map(|i| i.track.id.clone());
        }
        if self.repeat == Repeat::All {
            return self.context.first().map(|i| i.track.id.clone());
        }
        None
    }

    /// `skip_back` (FR-004): beyond 3 s (or empty history) restarts the
    /// current item; otherwise moves to the previous history entry.
    pub fn skip_back(&mut self, position_ms: u32) -> QueueChange {
        if position_ms > 3_000 {
            return QueueChange {
                order_changed: false,
                cursor_changed: false,
                playback: PlaybackChange::Restart,
            };
        }
        if let Some(&back) = self.history.back()
            && Some(back) == self.current.as_ref().map(|c| c.uid)
        {
            self.history.pop_back();
        }
        let Some(uid) = self.history.pop_back() else {
            return QueueChange {
                order_changed: false,
                cursor_changed: false,
                playback: PlaybackChange::Restart,
            };
        };
        if let Some(index) = self.find_context_index(uid) {
            self.context_pos = Some(index);
            let item = self.context[index].clone();
            self.current = Some(item);
            self.push_history(uid);
            return QueueChange {
                order_changed: false,
                cursor_changed: true,
                playback: PlaybackChange::MoveTo(uid),
            };
        }
        QueueChange {
            order_changed: false,
            cursor_changed: false,
            playback: PlaybackChange::Restart,
        }
    }

    /// Mark every item with `track_id` as unavailable (FR-026). If it is
    /// the current item, also skips forward past it.
    pub fn mark_unavailable(&mut self, track_id: &TrackId, rng: &mut impl QueueRng) -> QueueChange {
        let mut any = false;
        for item in self.context.iter_mut().chain(self.play_next.iter_mut()) {
            if &item.track.id == track_id {
                item.unavailable = true;
                any = true;
            }
        }
        if !any {
            return QueueChange::none();
        }
        if self
            .current
            .as_ref()
            .is_some_and(|c| &c.track.id == track_id)
        {
            return self.advance_internal(AdvanceReason::Removed, rng);
        }
        QueueChange {
            order_changed: false,
            cursor_changed: false,
            playback: PlaybackChange::None,
        }
    }

    /// When the current item is the only one left in this cycle and
    /// `repeat == Repeat::All`, the order `sync_program` should send ahead
    /// of the actual wrap: `[current, ...reshuffled_rest]` (reshuffled only
    /// when shuffle is on, natural context order otherwise), so the source
    /// can preload the first item of the next cycle for a gapless wrap
    /// (contracts/transport-and-queue.md §3, FR-013). `None` when a wrap
    /// preview does not apply (repeat isn't `All`, fewer than two context
    /// items, or there is still an upcoming item this cycle).
    pub fn wrap_preview(&self, rng: &mut impl QueueRng) -> Option<Vec<TrackId>> {
        if self.repeat != Repeat::All || self.context.len() < 2 {
            return None;
        }
        if !self.play_next.is_empty() || !self.upcoming_context_items().is_empty() {
            return None;
        }
        let current = self.current.as_ref()?;
        let mut rest: Vec<TrackId> = self
            .context
            .iter()
            .filter(|item| item.uid != current.uid)
            .map(|item| item.track.id.clone())
            .collect();
        if self.shuffle_order.is_some() {
            shuffle_in_place(&mut rest, rng);
        }
        let mut order = vec![current.track.id.clone()];
        order.append(&mut rest);
        Some(order)
    }

    /// The full effective order + cursor index, for `SourceCommand::
    /// LoadProgram` (data-model.md §2.3). `None` when there is nothing to
    /// play.
    pub fn program(&self) -> Option<QueueProgram> {
        self.current.as_ref()?;
        let order = self
            .effective_order()
            .into_iter()
            .map(|item| item.track.id.clone())
            .collect();
        Some(QueueProgram {
            order,
            cursor_index: 0,
        })
    }
}

// `Availability` is re-exported through `modplayer_audio_source` and used
// by callers constructing `TrackRef`s for this module's tests below.
#[allow(unused_imports)]
use Availability as _AvailabilityDocLink;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
mod tests {
    use super::*;

    fn track(id: &str) -> TrackRef {
        TrackRef::new(
            TrackId::new(format!("spotify:track:{id}")).unwrap_or_else(|_| unreachable!()),
            format!("Title {id}"),
            vec!["Artist".to_string()],
            None,
            None,
            180_000,
            Availability::Available,
        )
    }

    fn tracks(ids: &[&str]) -> Vec<TrackRef> {
        ids.iter().map(|id| track(id)).collect()
    }

    fn rng() -> XorShiftRng {
        XorShiftRng::from_seed(42)
    }

    #[test]
    fn replace_context_seeds_current_and_effective_order() {
        let mut q = Queue::new();
        let change = q.replace_context(tracks(&["a", "b", "c"]));
        assert_eq!(change.playback, PlaybackChange::Restart);
        assert_eq!(
            q.current().map(|i| i.track.id.as_str()),
            Some("spotify:track:a")
        );
        let order: Vec<_> = q
            .effective_order()
            .iter()
            .map(|i| i.track.id.as_str().to_string())
            .collect();
        assert_eq!(
            order,
            vec!["spotify:track:a", "spotify:track:b", "spotify:track:c"]
        );
    }

    #[test]
    fn empty_replace_context_is_empty_change() {
        let mut q = Queue::new();
        let change = q.replace_context(vec![]);
        assert_eq!(change.playback, PlaybackChange::Empty);
        assert!(q.is_empty());
        assert!(q.current().is_none());
    }

    #[test]
    fn advance_walks_natural_context_order() {
        let mut q = Queue::new();
        q.replace_context(tracks(&["a", "b", "c"]));
        let mut r = rng();

        let change = q.advance(AdvanceReason::TrackEnd, &mut r);
        assert_eq!(
            change.playback,
            PlaybackChange::MoveTo(q.current().unwrap().uid)
        );
        assert_eq!(q.current().unwrap().track.id.as_str(), "spotify:track:b");

        q.advance(AdvanceReason::TrackEnd, &mut r);
        assert_eq!(q.current().unwrap().track.id.as_str(), "spotify:track:c");

        let change = q.advance(AdvanceReason::TrackEnd, &mut r);
        assert_eq!(change.playback, PlaybackChange::EndOfQueue);
        assert!(q.current().is_none());
    }

    #[test]
    fn play_next_is_fifo_and_contiguous_after_current() {
        let mut q = Queue::new();
        q.replace_context(tracks(&["a", "b", "c"]));
        let mut r = rng();

        q.play_next_track(track("x"));
        q.play_next_track(track("y"));

        let order: Vec<_> = q
            .effective_order()
            .iter()
            .map(|i| i.track.id.as_str().to_string())
            .collect();
        assert_eq!(
            order,
            vec![
                "spotify:track:a",
                "spotify:track:x",
                "spotify:track:y",
                "spotify:track:b",
                "spotify:track:c"
            ]
        );

        let change = q.advance(AdvanceReason::TrackEnd, &mut r);
        assert_eq!(
            change.playback,
            PlaybackChange::MoveTo(q.current().unwrap().uid)
        );
        assert_eq!(q.current().unwrap().track.id.as_str(), "spotify:track:x");

        q.advance(AdvanceReason::TrackEnd, &mut r);
        assert_eq!(q.current().unwrap().track.id.as_str(), "spotify:track:y");

        // Play-next block drained: natural context resumes right after
        // wherever it left off (item "a").
        q.advance(AdvanceReason::TrackEnd, &mut r);
        assert_eq!(q.current().unwrap().track.id.as_str(), "spotify:track:b");
    }

    #[test]
    fn shuffle_keeps_current_and_visits_every_item_once_per_cycle() {
        let mut q = Queue::new();
        q.replace_context(tracks(&["a", "b", "c", "d"]));
        let mut r = rng();
        let before = q.current().unwrap().uid;
        q.set_shuffle(true, &mut r);
        assert!(q.shuffle_enabled());
        assert_eq!(q.current().unwrap().uid, before, "shuffle keeps current");

        let mut seen = vec![q.current().unwrap().track.id.as_str().to_string()];
        for _ in 0..3 {
            q.advance(AdvanceReason::TrackEnd, &mut r);
            seen.push(q.current().unwrap().track.id.as_str().to_string());
        }
        seen.sort();
        assert_eq!(
            seen,
            vec![
                "spotify:track:a",
                "spotify:track:b",
                "spotify:track:c",
                "spotify:track:d"
            ]
        );
    }

    #[test]
    fn repeat_all_wraps_and_reshuffles() {
        let mut q = Queue::new();
        q.replace_context(tracks(&["a", "b"]));
        q.set_repeat(Repeat::All);
        let mut r = rng();

        q.advance(AdvanceReason::TrackEnd, &mut r); // -> b
        let change = q.advance(AdvanceReason::TrackEnd, &mut r); // wraps -> a
        assert!(matches!(change.playback, PlaybackChange::MoveTo(_)));
        assert_eq!(q.current().unwrap().track.id.as_str(), "spotify:track:a");
    }

    #[test]
    fn repeat_one_restarts_on_track_end_only() {
        let mut q = Queue::new();
        q.replace_context(tracks(&["a", "b"]));
        q.set_repeat(Repeat::One);
        let mut r = rng();

        let change = q.advance(AdvanceReason::TrackEnd, &mut r);
        assert_eq!(change.playback, PlaybackChange::Restart);
        assert_eq!(q.current().unwrap().track.id.as_str(), "spotify:track:a");

        // Skip ignores repeat-one.
        let change = q.advance(AdvanceReason::Skip, &mut r);
        assert!(matches!(change.playback, PlaybackChange::MoveTo(_)));
        assert_eq!(q.current().unwrap().track.id.as_str(), "spotify:track:b");
    }

    #[test]
    fn unavailable_item_is_skipped_by_advance() {
        let mut q = Queue::new();
        q.replace_context(tracks(&["a", "b", "c"]));
        let mut r = rng();
        let b_id = TrackId::new("spotify:track:b").unwrap_or_else(|_| unreachable!());
        q.mark_unavailable(&b_id, &mut r);

        let change = q.advance(AdvanceReason::TrackEnd, &mut r);
        assert!(matches!(change.playback, PlaybackChange::Skipped(_)));
        let change = q.advance(AdvanceReason::TrackEnd, &mut r);
        assert_eq!(
            change.playback,
            PlaybackChange::MoveTo(q.current().unwrap().uid)
        );
        assert_eq!(q.current().unwrap().track.id.as_str(), "spotify:track:c");
    }

    #[test]
    fn mark_unavailable_on_current_skips_immediately() {
        let mut q = Queue::new();
        q.replace_context(tracks(&["a", "b"]));
        let mut r = rng();
        let a_id = TrackId::new("spotify:track:a").unwrap_or_else(|_| unreachable!());
        let change = q.mark_unavailable(&a_id, &mut r);
        assert!(matches!(change.playback, PlaybackChange::MoveTo(_)));
        assert_eq!(q.current().unwrap().track.id.as_str(), "spotify:track:b");
    }

    #[test]
    fn skip_back_within_3s_moves_to_previous_history_entry() {
        let mut q = Queue::new();
        q.replace_context(tracks(&["a", "b", "c"]));
        let mut r = rng();
        q.advance(AdvanceReason::TrackEnd, &mut r); // -> b
        q.advance(AdvanceReason::TrackEnd, &mut r); // -> c

        let change = q.skip_back(1_000);
        assert!(matches!(change.playback, PlaybackChange::MoveTo(_)));
        assert_eq!(q.current().unwrap().track.id.as_str(), "spotify:track:b");
    }

    #[test]
    fn skip_back_beyond_3s_restarts_current() {
        let mut q = Queue::new();
        q.replace_context(tracks(&["a", "b"]));
        let mut r = rng();
        q.advance(AdvanceReason::TrackEnd, &mut r); // -> b

        let change = q.skip_back(5_000);
        assert_eq!(change.playback, PlaybackChange::Restart);
        assert_eq!(q.current().unwrap().track.id.as_str(), "spotify:track:b");
    }

    #[test]
    fn skip_back_with_empty_history_restarts_current() {
        let mut q = Queue::new();
        q.replace_context(tracks(&["a"]));
        let change = q.skip_back(500);
        assert_eq!(change.playback, PlaybackChange::Restart);
    }

    #[test]
    fn history_is_bounded_at_100() {
        let mut q = Queue::new();
        let many: Vec<String> = (0..150).map(|i| i.to_string()).collect();
        let ids: Vec<&str> = many.iter().map(String::as_str).collect();
        q.replace_context(tracks(&ids));
        q.set_repeat(Repeat::All);
        let mut r = rng();
        for _ in 0..200 {
            q.advance(AdvanceReason::TrackEnd, &mut r);
        }
        assert!(q.history.len() <= HISTORY_LIMIT);
    }

    #[test]
    fn move_up_crosses_into_play_next_block() {
        let mut q = Queue::new();
        q.replace_context(tracks(&["a", "b", "c"]));
        q.play_next_track(track("x"));

        // Effective order: a, x, b, c. Move "b" up one slot: a, b, x, c —
        // "b" crosses into the play-next block, "x" crosses out.
        let b_uid = q
            .effective_order()
            .iter()
            .find(|i| i.track.id.as_str() == "spotify:track:b")
            .map(|i| i.uid)
            .unwrap_or_else(|| unreachable!());
        q.move_up(b_uid);

        let order: Vec<_> = q
            .effective_order()
            .iter()
            .map(|i| i.track.id.as_str().to_string())
            .collect();
        assert_eq!(
            order,
            vec![
                "spotify:track:a",
                "spotify:track:b",
                "spotify:track:x",
                "spotify:track:c"
            ]
        );
    }

    #[test]
    fn remove_current_advances_past_it() {
        let mut q = Queue::new();
        q.replace_context(tracks(&["a", "b"]));
        let current = q.current().unwrap().uid;
        let change = q.remove(current);
        assert!(matches!(change.playback, PlaybackChange::MoveTo(_)));
        assert_eq!(q.current().unwrap().track.id.as_str(), "spotify:track:b");
    }

    #[test]
    fn program_reflects_effective_order() {
        let mut q = Queue::new();
        q.replace_context(tracks(&["a", "b"]));
        let program = q.program().unwrap_or_else(|| unreachable!());
        assert_eq!(program.cursor_index, 0);
        assert_eq!(program.order.len(), 2);
    }

    #[test]
    fn program_is_none_when_empty() {
        let q = Queue::new();
        assert!(q.program().is_none());
    }

    #[test]
    fn ensure_host_driven_forces_mode_even_when_source_driven() {
        let mut q = Queue::new();
        q.replace_context(tracks(&["a"]));
        q.mode = QueueMode::SourceDriven;
        assert_eq!(q.mode(), QueueMode::SourceDriven);
        q.ensure_host_driven();
        assert_eq!(q.mode(), QueueMode::HostDriven);
    }

    #[test]
    fn wrap_preview_is_none_off_the_last_item_of_the_cycle() {
        let mut q = Queue::new();
        q.replace_context(tracks(&["a", "b"]));
        q.set_repeat(Repeat::All);
        let mut r = rng();
        // Still on "a" with "b" upcoming: no wrap preview yet.
        assert!(q.wrap_preview(&mut r).is_none());
    }

    #[test]
    fn wrap_preview_previews_the_next_cycle_from_the_last_item() {
        let mut q = Queue::new();
        q.replace_context(tracks(&["a", "b", "c"]));
        q.set_repeat(Repeat::All);
        q.set_shuffle(true, &mut rng());
        let mut r = rng();
        q.advance(AdvanceReason::TrackEnd, &mut r);
        q.advance(AdvanceReason::TrackEnd, &mut r);
        // Now on the last item of the cycle.
        let current_id = q.current().unwrap().track.id.clone();

        let preview = q.wrap_preview(&mut r).unwrap_or_else(|| unreachable!());
        assert_eq!(preview.len(), 3);
        assert_eq!(preview[0], current_id, "the last item stays first");
        let mut rest = preview[1..].to_vec();
        rest.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        let mut expected: Vec<_> = tracks(&["a", "b", "c"])
            .into_iter()
            .map(|t| t.id)
            .filter(|id| id != &current_id)
            .collect();
        expected.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        assert_eq!(rest, expected);
    }

    #[test]
    fn item_finds_an_item_wherever_it_lives() {
        let mut q = Queue::new();
        q.replace_context(tracks(&["a", "b"]));
        q.play_next_track(track("x"));
        let current_uid = q.current().unwrap().uid;
        let x_uid = q
            .play_next_items()
            .next()
            .unwrap_or_else(|| unreachable!())
            .uid;

        assert_eq!(
            q.item(current_uid).map(|i| i.track.id.as_str().to_string()),
            Some("spotify:track:a".to_string())
        );
        assert_eq!(
            q.item(x_uid).map(|i| i.track.id.as_str().to_string()),
            Some("spotify:track:x".to_string())
        );
        assert!(q.item(QueueItemId(9_999)).is_none());
    }

    #[test]
    fn adopt_transfer_context_seeds_a_single_source_driven_item() {
        let mut q = Queue::new();
        q.replace_context(tracks(&["a", "b"]));
        assert_eq!(q.mode(), QueueMode::HostDriven);

        let change = q.adopt_transfer_context(track("z"));
        assert!(matches!(change.playback, PlaybackChange::MoveTo(_)));
        assert_eq!(q.mode(), QueueMode::SourceDriven);
        assert_eq!(q.current().unwrap().track.id.as_str(), "spotify:track:z");
        assert_eq!(q.current().unwrap().origin, Origin::Revealed);
        assert_eq!(q.effective_order().len(), 1, "the old context is discarded");

        // Growth via `reveal()`, exactly like a natural source-driven
        // session (research R3).
        q.reveal(track("y"));
        assert_eq!(q.effective_order().len(), 2);
    }

    #[test]
    fn replace_context_at_starts_on_the_clicked_track() {
        let mut q = Queue::new();
        let change = q.replace_context_at(tracks(&["a", "b", "c"]), 1);
        assert_eq!(change.playback, PlaybackChange::Restart);
        assert_eq!(q.current().unwrap().track.id.as_str(), "spotify:track:b");
        let order: Vec<_> = q
            .effective_order()
            .iter()
            .map(|i| i.track.id.as_str().to_string())
            .collect();
        assert_eq!(
            order,
            vec!["spotify:track:b", "spotify:track:c"],
            "context still holds only the upcoming items from cursor onward"
        );
    }

    #[test]
    fn replace_context_at_clamps_a_cursor_past_the_end() {
        let mut q = Queue::new();
        q.replace_context_at(tracks(&["a", "b"]), 99);
        assert_eq!(q.current().unwrap().track.id.as_str(), "spotify:track:b");
    }

    #[test]
    fn replace_context_at_zero_behaves_like_replace_context() {
        let mut q = Queue::new();
        let change = q.replace_context_at(tracks(&["a", "b"]), 0);
        assert_eq!(change.playback, PlaybackChange::Restart);
        assert_eq!(q.current().unwrap().track.id.as_str(), "spotify:track:a");
    }

    #[test]
    fn replace_context_at_with_no_tracks_is_empty_change() {
        let mut q = Queue::new();
        let change = q.replace_context_at(vec![], 0);
        assert_eq!(change.playback, PlaybackChange::Empty);
        assert!(q.is_empty());
    }

    #[test]
    fn play_next_tracks_preserves_order_at_the_tail() {
        let mut q = Queue::new();
        q.replace_context(tracks(&["a", "b"]));
        q.play_next_tracks(tracks(&["x", "y", "z"]));
        let order: Vec<_> = q
            .effective_order()
            .iter()
            .map(|i| i.track.id.as_str().to_string())
            .collect();
        assert_eq!(
            order,
            vec![
                "spotify:track:a",
                "spotify:track:x",
                "spotify:track:y",
                "spotify:track:z",
                "spotify:track:b",
            ]
        );
    }

    #[test]
    fn play_next_tracks_on_an_empty_queue_auto_starts_the_first() {
        let mut q = Queue::new();
        let change = q.play_next_tracks(tracks(&["x", "y"]));
        assert!(matches!(change.playback, PlaybackChange::MoveTo(_)));
        assert_eq!(q.current().unwrap().track.id.as_str(), "spotify:track:x");
        let order: Vec<_> = q
            .effective_order()
            .iter()
            .map(|i| i.track.id.as_str().to_string())
            .collect();
        assert_eq!(order, vec!["spotify:track:x", "spotify:track:y"]);
    }

    #[test]
    fn play_next_tracks_with_no_tracks_is_a_no_op() {
        let mut q = Queue::new();
        q.replace_context(tracks(&["a"]));
        let change = q.play_next_tracks(vec![]);
        assert_eq!(change, QueueChange::none());
    }

    #[test]
    fn track_title_finds_the_first_matching_track() {
        let mut q = Queue::new();
        q.replace_context(tracks(&["a", "b"]));
        let b_id = TrackId::new("spotify:track:b").unwrap_or_else(|_| unreachable!());
        assert_eq!(q.track_title(&b_id), Some("Title b".to_string()));
        let missing = TrackId::new("spotify:track:zzz").unwrap_or_else(|_| unreachable!());
        assert_eq!(q.track_title(&missing), None);
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! The event queue: advisory notifications pushed from the `Processor` to
//! the `PlaybackController` (contracts/engine-commands.md). Push never
//! blocks; on overflow the event is dropped — all state of record lives in
//! `RtShared` atomics or controller shadow state, never only in an event.

/// An event pushed from a running `Processor` to the `PlaybackController`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// The test tone completed in the buffer this event was pushed for.
    ToneFinished,
    /// The source position wrapped to 0 while playing.
    TrackLooped { at_clock: u64 },
    /// Defensive: the command drain found an unknown discriminant. Never
    /// expected in practice.
    CommandDropped { kind: u8 },
    /// A loop wrap happened in the render that pushed this event (006,
    /// contracts/engine-loop.md §3): `wraps` is the post-increment count;
    /// `gapless` is whether the seam's incoming frames were all read from
    /// the store *and* the store covered `A` (a hard cut otherwise).
    LoopWrapped { wraps: u32, gapless: bool },
    /// The armed region's repeat count was reached right after the wrap
    /// that reached it; `loop_active` is already `None` by the time this
    /// is pushed (006, contracts/engine-loop.md §3).
    LoopReleased { wraps: u32 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_is_copy() {
        fn assert_copy<T: Copy>() {}
        assert_copy::<Event>();
    }
}

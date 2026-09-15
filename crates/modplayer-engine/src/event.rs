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

// SPDX-License-Identifier: MIT OR Apache-2.0

//! The command queue: the only write path into a running `Processor`
//! (contracts/engine-commands.md). Every setter is a `Command`; there is no
//! `&mut Processor` reachable from outside the audio callback.

use crate::types::{CeilingDb, VolumePercent};

/// A command sent from the `PlaybackController` to a running `Processor`.
///
/// `Copy`, at most 16 bytes, no heap — pushed through an `rtrb` SPSC queue
/// and drained in full at the start of every `render` call (buffer
/// boundary). There is intentionally no command to bypass, remove, or
/// raise the limiter above `CeilingDb::MAX`, and no command to change the
/// source or output stage (FR-009; those are construction-time parameters
/// on `ProcessorConfig`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command {
    /// Set the master gain, applied at the next buffer boundary.
    SetMasterVolume(VolumePercent),
    /// Set the limiter ceiling, applied (and re-clamped) at the next buffer
    /// boundary.
    SetCeiling(CeilingDb),
    /// Transition transport to `Playing`; position continues from current.
    Play,
    /// Transition transport to `Paused`; position retained.
    Pause,
    /// Transition transport to `Stopped`; seeks the source to 0.
    Stop,
    /// (Re)start the test tone from its fade-in, independent of transport.
    PlayTestTone,
}

const _: () = assert!(
    std::mem::size_of::<Command>() <= 16,
    "Command must stay <= 16 bytes for the real-time SPSC queue"
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_is_copy_and_small() {
        fn assert_copy<T: Copy>() {}
        assert_copy::<Command>();
        assert!(std::mem::size_of::<Command>() <= 16);
    }
}

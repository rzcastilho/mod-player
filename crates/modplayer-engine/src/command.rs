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
    /// Seek the source to `frame` at the next buffer boundary
    /// (engine-delta.md §1); position atomics reflect it after that
    /// render.
    Seek(u64),
    /// Stage the loop region's `A` endpoint, in source frames (006,
    /// contracts/engine-loop.md §2). Applied to `loop_staged` at the next
    /// buffer boundary; takes effect only once a `LoopCommit` follows.
    LoopSetA(u64),
    /// Stage the loop region's `B` endpoint, in source frames (006,
    /// contracts/engine-loop.md §2).
    LoopSetB(u64),
    /// Stage the loop region's configured crossfade (in source frames, not
    /// yet reduced by `loop_math::effective_crossfade`) and repeat count
    /// (`0` = infinite) (006, contracts/engine-loop.md §2).
    LoopSetSeam { crossfade_frames: u32, repeat: u32 },
    /// Atomically swap `loop_staged` into `loop_active` (006, contracts/
    /// engine-loop.md §2, FR-011a): `reset_wraps` zeroes the wrap count
    /// (arming) or keeps it (editing a region already armed). A seam
    /// already in progress finishes with its captured bounds regardless.
    LoopCommit { reset_wraps: bool },
    /// Disarm the loop: `loop_active = None`; a seam already in progress
    /// still finishes, but without jumping at `B` (006, contracts/
    /// engine-loop.md §2).
    LoopDisarm,
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

    /// contracts/engine-loop.md §2: the five loop variants stay within the
    /// same `Copy`, <= 16-byte contract as every other `Command` (T032).
    #[test]
    fn loop_commands_are_copy_and_small() {
        let commands = [
            Command::LoopSetA(1_234),
            Command::LoopSetB(5_678),
            Command::LoopSetSeam {
                crossfade_frames: 2_205,
                repeat: 3,
            },
            Command::LoopCommit { reset_wraps: true },
            Command::LoopDisarm,
        ];
        for command in commands {
            let _copy = command;
        }
        assert!(std::mem::size_of::<Command>() <= 16);
    }
}

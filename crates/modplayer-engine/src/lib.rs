// SPDX-License-Identifier: MIT OR Apache-2.0
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used)]

//! The real-time processor: the only code in ModPlayer permitted to run on
//! the audio callback thread (Constitution Principle I, non-negotiable).
//!
//! `render()` and everything it calls must never allocate, lock, block, do
//! I/O, or log.

pub mod command;
pub mod event;
pub mod limiter;
pub mod loop_math;
pub mod output_stage;
pub mod position_clock;
pub mod processor;
pub mod resample;
pub mod shared;
pub mod types;

pub use command::Command;
pub use event::Event;
pub use limiter::Limiter;
pub use output_stage::OutputStage;
pub use position_clock::PositionClock;
pub use processor::{Processor, ProcessorConfig};
pub use shared::{AnchorSnapshot, RtShared};
pub use types::{
    BufferPreset, CeilingDb, DeviceId, FrameCount, NegotiatedBuffer, SafeVolume, SampleRate, Theme,
    Transport, VolumePercent,
};

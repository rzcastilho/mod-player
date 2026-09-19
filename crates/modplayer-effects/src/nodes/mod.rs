// SPDX-License-Identifier: MIT OR Apache-2.0

//! Built-in DSP kernels (contracts/engine-effect-chain.md §5). Each
//! kernel processes a stereo-interleaved buffer in place, allocation-free,
//! driven by [`crate::smooth::Smoothed`] parameters.

pub mod eq;
pub mod filter;
pub mod gain;
pub mod lpc;
pub mod stereo;
pub mod stretch;

pub use eq::Eq8Dsp;
pub use filter::FilterDsp;
pub use gain::GainDsp;
pub use stereo::StereoDsp;
pub use stretch::StretchStage;

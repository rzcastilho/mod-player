// SPDX-License-Identifier: MIT OR Apache-2.0

//! The real-time chain runtime, owned by `Processor` (data-model.md §3).

pub mod chain;
pub mod cost;
pub mod slot;

pub use chain::{ChainRt, OverloadOutcome};
pub use cost::CostRing;
pub use slot::{MAX_PARAMS, NodeDsp, NodeSlot};

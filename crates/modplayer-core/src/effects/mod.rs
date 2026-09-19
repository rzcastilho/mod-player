// SPDX-License-Identifier: MIT OR Apache-2.0

//! The controller's effect-chain shadow state and its UI projections
//! (008-effect-chain-and-built-in-nodes, data-model.md §2).

pub mod model;
pub mod view;

pub use model::{ChainError, ChainModel, NodeId, NodeModel};
pub use view::{ChainView, LevelPair, MeterSnapshot, NodeRow};

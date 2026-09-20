// SPDX-License-Identifier: MIT OR Apache-2.0

//! The per-plugin key-value state store: paths, the atomic background
//! writer, and the in-memory store itself (DM-12, FR-014, FR-021,
//! data-model.md §1.7).

pub mod paths;
pub mod store;
pub mod writer;

pub use paths::PluginStatePaths;
pub use store::{PluginStateEntry, PluginStateStore, Scope, StoreError};
pub use writer::{StateWriter, WriteJob};

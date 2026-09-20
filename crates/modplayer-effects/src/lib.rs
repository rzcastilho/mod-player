// SPDX-License-Identifier: MIT OR Apache-2.0
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used)]

//! Built-in effect nodes and the allocation-free effect chain runtime
//! (008-effect-chain-and-built-in-nodes).
//!
//! See `specs/008-effect-chain-and-built-in-nodes/contracts/` and
//! `data-model.md` for the parameter catalog, chain runtime and node
//! contracts this crate implements.

pub mod biquad;
pub mod catalog;
pub mod consts;
pub mod crossfade;
pub mod nodes;
pub mod rt;
pub mod smooth;
pub mod spectrum;

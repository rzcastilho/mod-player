// SPDX-License-Identifier: MIT OR Apache-2.0
#![forbid(unsafe_code)]

//! The plugin runtime: one Luau VM per plugin, its own scheduler thread,
//! CPU/memory budgets, and the Lua<->Capability Gateway bindings
//! (009-plugin-runtime-and-permissions, research R1, R3-R5).
//!
//! See `specs/009-plugin-runtime-and-permissions/contracts/
//! gateway-and-runtime.md` and `data-model.md` §2 for the types and
//! rules this crate implements.

pub mod bindings;
pub mod budget;
pub mod context;
pub mod events;
pub mod handle;
pub mod pump;
pub mod scheduler;
pub mod timers;

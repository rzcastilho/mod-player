// SPDX-License-Identifier: MIT OR Apache-2.0
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used)]

//! The Capability Gateway: the one path every plugin request and event
//! goes through (009-plugin-runtime-and-permissions, Constitution II).
//!
//! See `specs/009-plugin-runtime-and-permissions/contracts/
//! gateway-and-runtime.md` and `data-model.md` §1 for the types and
//! rules this crate implements. This crate has no workspace dependency:
//! its API definition, manifest validator and state store are testable
//! alone and reusable by later registry tooling (plan.md § Project
//! Structure).

pub mod api;
pub mod budgets;
pub mod event;
pub mod focus;
pub mod gateway;
pub mod grants;
pub mod limiter;
pub mod manifest;
pub mod refusal;
pub mod request;
pub mod state;

// SPDX-License-Identifier: MIT OR Apache-2.0
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used)]

//! The `OutputBackend` trait plus its two implementors: `CpalBackend` (real
//! hardware, the only crate that imports `cpal`; lands in US1 T046) and
//! `FakeBackend` (headless CI). See `contracts/output-backend.md`.

pub mod backend;
pub mod cpal_backend;
pub mod error;
pub mod fake_backend;
pub mod watcher;

pub use backend::{
    BackendEvent, OpenStream, OutputBackend, OutputDeviceInfo, StreamHandle, StreamRequest,
};
pub use cpal_backend::CpalBackend;
pub use error::AudioIoError;
pub use fake_backend::{FakeBackend, FakeDevice};
pub use watcher::DeviceWatcher;

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Errors returned by an `OutputBackend` (contracts/output-backend.md).

use modplayer_engine::DeviceId;

/// Errors an `OutputBackend` implementation can return. Never returned from
/// the real-time path — only from enumeration/`open` calls.
#[derive(Debug, thiserror::Error)]
pub enum AudioIoError {
    #[error("no such output device: {0}")]
    NoSuchDevice(DeviceId),
    #[error("no output devices are available")]
    NoOutputDevices,
    #[error("output device unavailable: {0}")]
    DeviceUnavailable(DeviceId),
    #[error("unsupported stream configuration for {device}: {reason}")]
    UnsupportedConfig { device: DeviceId, reason: String },
    #[error("backend error: {0}")]
    Backend(String),
}

// SPDX-License-Identifier: MIT OR Apache-2.0
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used)]

//! Off-real-time-path services: the `PlaybackController` (single authority
//! for shadow state), settings store, notifications, i18n, device policy
//! and the settings registry/search.

pub mod controller;
pub mod device_policy;
pub mod i18n;
pub mod notifications;
pub mod settings;
pub mod settings_registry;

pub use controller::{ActiveDevice, PlaybackController};
pub use device_policy::{DeviceResolution, DeviceWarning};
pub use i18n::{tr, tr_args};
pub use notifications::{Notification, NotificationAction, NotificationCenter, Severity};
pub use settings::{AudioSettings, DisclosureAcknowledgement, SettingsStore};
pub use settings_registry::{SettingDescriptor, SettingsCategory};

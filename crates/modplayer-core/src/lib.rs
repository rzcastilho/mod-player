// SPDX-License-Identifier: MIT OR Apache-2.0
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used)]

//! Off-real-time-path services: the `PlaybackController` (single authority
//! for shadow state), settings store, notifications, i18n, device policy
//! and the settings registry/search.

pub mod controller;
pub mod device_policy;
pub mod i18n;
pub mod library;
pub mod links;
pub mod notifications;
pub mod queue;
pub mod search;
pub mod settings;
pub mod settings_registry;
pub mod transport;

pub use controller::{ActiveDevice, PlaybackController, QueueRow, QueueView, TrackListState};
pub use device_policy::{DeviceResolution, DeviceWarning};
pub use i18n::{tr, tr_args};
pub use library::{Connectivity, LibraryIndex, LibraryStatus, PlayLog, SyncScheduler};
pub use links::STATUS_PAGE_URL;
pub use notifications::{Notification, NotificationAction, NotificationCenter, Severity};
pub use queue::{
    AdvanceReason, Origin, PlaybackChange, Queue, QueueChange, QueueItem, QueueItemId, QueueMode,
    QueueProgram, QueueRng, XorShiftRng,
};
pub use search::{GROUP_ORDER, GroupState, SearchSession};
pub use settings::{AudioSettings, DeviceName, DisclosureAcknowledgement, SettingsStore};
pub use settings_registry::{SettingDescriptor, SettingsCategory};
pub use transport::{
    ActiveState, Intent, NotRegisteredReason, PendingTransferCommand, TransportState,
};

// SPDX-License-Identifier: MIT OR Apache-2.0
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used)]

//! Off-real-time-path services: the `PlaybackController` (single authority
//! for shadow state), settings store, notifications, i18n, device policy
//! and the settings registry/search.

pub mod actions;
pub mod analysis;
pub mod controller;
pub mod device_policy;
pub mod effects;
pub mod i18n;
pub mod library;
pub mod links;
pub mod markers;
pub mod notifications;
pub mod plugins;
pub mod queue;
pub mod search;
pub mod settings;
pub mod settings_registry;
pub mod transport;

pub use actions::{
    ActionCategory, ActionDef, ActionKind, ActionOwner, ActionRegistry, ActionRow, BindingError,
    CATALOG, Chord, ChordParseError, HostAction, KEY_NAMES, KeyName, KeymapOverrides, Mods,
    Platform, Scope, ScopeState, def,
};
pub use analysis::{
    ANALYZER_VERSION, AnalysisPaths, AnalysisService, AnalysisSnapshot, AnalysisStatus, PeakLevel,
    WaveformPeaks,
};
pub use controller::{
    ActiveDevice, LoopState, LoopStatus, NowPlayingPanel, PlaybackController, QueueRow, QueueView,
    TrackListState,
};
pub use device_policy::{DeviceResolution, DeviceWarning};
pub use effects::{
    ChainError, ChainModel, ChainView, LevelPair, MeterSnapshot, NodeId, NodeModel, NodeRow,
};
pub use i18n::{tr, tr_args};
pub use library::{Connectivity, LibraryIndex, LibraryStatus, PlayLog, SyncScheduler};
pub use links::{GETTING_STARTED_TUTORIAL_URL, STATUS_PAGE_URL};
pub use modplayer_capability_gateway::refusal::Refusal;
pub use notifications::{
    Notification, NotificationAction, NotificationCenter, PluginAttribution, Severity,
};
pub use plugins::{
    FocusHolder, FocusPolicy, FocusRow, Health, Lifecycle, PluginAssets, PluginHost, PluginId,
    PluginLog, PluginRecord, PluginRow, PluginUi, PluginsView, Source, TransportFocusView,
};
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

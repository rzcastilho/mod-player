// SPDX-License-Identifier: MIT OR Apache-2.0

//! `HostEvent`: what the host delivers to a plugin's registered handlers
//! (data-model.md §1.5, contracts/plugin-api-v1.md §4, FR-016, FR-018,
//! FR-019, FR-020, FR-028).

use std::collections::BTreeMap;

use crate::api::{ApiVersion, Permission};
use crate::request::{NodeInfo, OwnerInfo, QueueItemInfo, RegionId};
use crate::ui::WidgetValue;

/// Who invoked a plugin shortcut action (contract §4's `action_invoked`
/// payload). `"midi"` is reserved for a later slice and never produced
/// this one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionSource {
    Keyboard,
    Ui,
}

/// Why the plugin's thread is being told to unload (contract §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnloadReason {
    Disable,
    Suspend,
    Shutdown,
}

/// The transport's coarse playback state, as seen by a plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayState {
    Playing,
    Paused,
    Stopped,
}

/// The current track's plugin-visible metadata (data-model.md §1.5).
#[derive(Debug, Clone, PartialEq)]
pub struct TrackInfo {
    pub id: String,
    pub title: String,
    pub artists: Vec<String>,
    pub duration_ms: u64,
}

/// One side (pre/post) of a `meter` event's level pair (contract §4).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LevelInfo {
    pub peak_l: f32,
    pub peak_r: f32,
    pub rms_l: f32,
    pub rms_r: f32,
}

/// A `TimerHandle` as seen from the gateway crate (the runtime crate's own
/// `TimerHandle` mirrors this 1:1; kept separate so this crate has no
/// dependency on the runtime).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TimerHandle(pub u32);

/// Every event the host can deliver to a plugin (data-model.md §1.5),
/// exactly the [`crate::api::EventKind`] set with payload attached.
#[derive(Debug, Clone, PartialEq)]
pub enum HostEvent {
    ReadyAck {
        api: ApiVersion,
        granted: Vec<Permission>,
        capabilities: Vec<&'static str>,
    },
    Unloading {
        reason: UnloadReason,
    },
    TrackChanged {
        track: Option<TrackInfo>,
    },
    Position {
        position_ms: u64,
    },
    PlayStateChanged {
        state: PlayState,
    },
    QueueChanged {
        items: Vec<QueueItemInfo>,
    },
    MarkerChanged {
        actor: OwnerInfo,
        revision: u64,
    },
    LoopArmed {
        region: RegionId,
        by: OwnerInfo,
    },
    LoopDisarmed {
        by: OwnerInfo,
    },
    LoopWrapped {
        region: RegionId,
        wraps: u32,
    },
    EffectChainChanged {
        chain: Vec<NodeInfo>,
    },
    Meter {
        pre: LevelInfo,
        post: LevelInfo,
        /// Boxed to keep [`HostEvent`] small (`clippy::large_enum_variant`)
        /// — this variant is the only one carrying per-frame spectrum data.
        spectrum: Box<[f32; 64]>,
    },
    Timer {
        handle: TimerHandle,
    },
    PositionReached {
        handle: TimerHandle,
        position_ms: u64,
    },
    FocusGranted {
        holder: OwnerInfo,
    },
    FocusRevoked {
        holder: OwnerInfo,
    },
    /// 011-plugin-ui-contributions (contract §4): a committed change to a
    /// widget the plugin owns; delivered direct to the handle, once per
    /// committed change.
    PanelInteraction {
        panel: String,
        widget: String,
        value: WidgetValue,
    },
    /// A plugin shortcut action fired (keyboard or a panel button);
    /// `value` is `Some(1.0)` for `Continuous`, `None` for `Trigger`.
    ActionInvoked {
        action: String,
        source: ActionSource,
        value: Option<f64>,
    },
    /// A user edit to this plugin's settings page landed in its store,
    /// delivered **after** the value is already readable through
    /// `get_settings()` (R5).
    SettingsChanged {
        changes: BTreeMap<String, serde_json::Value>,
    },
}

impl HostEvent {
    /// The [`crate::api::EventKind`] this event carries, for the fan-out
    /// permission filter (C4).
    #[must_use]
    pub const fn kind(&self) -> crate::api::EventKind {
        use crate::api::EventKind;
        match self {
            HostEvent::ReadyAck { .. } => EventKind::ReadyAck,
            HostEvent::Unloading { .. } => EventKind::Unloading,
            HostEvent::TrackChanged { .. } => EventKind::TrackChanged,
            HostEvent::Position { .. } => EventKind::Position,
            HostEvent::PlayStateChanged { .. } => EventKind::PlayStateChanged,
            HostEvent::QueueChanged { .. } => EventKind::QueueChanged,
            HostEvent::MarkerChanged { .. } => EventKind::MarkerChanged,
            HostEvent::LoopArmed { .. } => EventKind::LoopArmed,
            HostEvent::LoopDisarmed { .. } => EventKind::LoopDisarmed,
            HostEvent::LoopWrapped { .. } => EventKind::LoopWrapped,
            HostEvent::EffectChainChanged { .. } => EventKind::EffectChainChanged,
            HostEvent::Meter { .. } => EventKind::Meter,
            HostEvent::Timer { .. } => EventKind::Timer,
            HostEvent::PositionReached { .. } => EventKind::PositionReached,
            HostEvent::FocusGranted { .. } => EventKind::FocusGranted,
            HostEvent::FocusRevoked { .. } => EventKind::FocusRevoked,
            HostEvent::PanelInteraction { .. } => EventKind::PanelInteraction,
            HostEvent::ActionInvoked { .. } => EventKind::ActionInvoked,
            HostEvent::SettingsChanged { .. } => EventKind::SettingsChanged,
        }
    }
}

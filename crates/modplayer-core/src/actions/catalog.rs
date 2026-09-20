// SPDX-License-Identifier: MIT OR Apache-2.0

//! `HostAction`, `ActionDef` and the 46-entry default catalog (007,
//! data-model.md §1.1-1.2, spec § Default Action Catalog; 008 appends
//! `ToggleEffectChain`, contracts/effects-service.md §4; 010-transport-
//! focus appends `ToggleTransportPanel`, data-model.md §2.3).

use super::{ActionCategory, ActionKind, ActionOwner, Scope};
use crate::markers::CueSlot;

/// A named, dispatchable host operation (DM-14). Exactly 46 variants,
/// in the catalog's display order; ids are append-only across slices
/// (never renumbered or removed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum HostAction {
    // Transport
    Play,
    Pause,
    TogglePlayPause,
    Stop,
    NextTrack,
    PreviousTrack,
    SeekForwardStep,
    SeekBackwardStep,
    VolumeUp,
    VolumeDown,
    // Markers
    AddPointMarker,
    SetA,
    SetB,
    NudgeEarlier,
    NudgeLater,
    NudgeEarlierX10,
    NudgeLaterX10,
    ClearAllMarkers,
    // Loop
    ToggleLoop,
    // Cues
    SetCue(CueSlot),
    JumpToCue(CueSlot),
    // Navigation
    NavLibrary,
    NavSearch,
    NavNowPlaying,
    NavPlugins,
    NavSettings,
    ToggleQueue,
    FocusSearch,
    // Effects
    TempoStepUp,
    TempoStepDown,
    // 008, contracts/effects-service.md §4: appended last (append-only
    // ids) even though its category is Navigation, so no earlier
    // variant's id/position shifts.
    ToggleEffectChain,
    // 010-transport-focus, data-model.md §2.3: appended last for the same
    // reason.
    ToggleTransportPanel,
}

/// `n = 1..=8`, in that order, via [`CueSlot::new_const`] (no `Option`
/// unwrap needed for a `const` array of compile-time-known-valid slots).
const CUE_SLOTS: [CueSlot; 8] = [
    CueSlot::new_const(1),
    CueSlot::new_const(2),
    CueSlot::new_const(3),
    CueSlot::new_const(4),
    CueSlot::new_const(5),
    CueSlot::new_const(6),
    CueSlot::new_const(7),
    CueSlot::new_const(8),
];

impl HostAction {
    /// Every action, in catalog order (data-model.md §1.1).
    pub const ALL: [HostAction; 46] = [
        HostAction::Play,
        HostAction::Pause,
        HostAction::TogglePlayPause,
        HostAction::Stop,
        HostAction::NextTrack,
        HostAction::PreviousTrack,
        HostAction::SeekForwardStep,
        HostAction::SeekBackwardStep,
        HostAction::VolumeUp,
        HostAction::VolumeDown,
        HostAction::AddPointMarker,
        HostAction::SetA,
        HostAction::SetB,
        HostAction::NudgeEarlier,
        HostAction::NudgeLater,
        HostAction::NudgeEarlierX10,
        HostAction::NudgeLaterX10,
        HostAction::ClearAllMarkers,
        HostAction::ToggleLoop,
        HostAction::SetCue(CUE_SLOTS[0]),
        HostAction::SetCue(CUE_SLOTS[1]),
        HostAction::SetCue(CUE_SLOTS[2]),
        HostAction::SetCue(CUE_SLOTS[3]),
        HostAction::SetCue(CUE_SLOTS[4]),
        HostAction::SetCue(CUE_SLOTS[5]),
        HostAction::SetCue(CUE_SLOTS[6]),
        HostAction::SetCue(CUE_SLOTS[7]),
        HostAction::JumpToCue(CUE_SLOTS[0]),
        HostAction::JumpToCue(CUE_SLOTS[1]),
        HostAction::JumpToCue(CUE_SLOTS[2]),
        HostAction::JumpToCue(CUE_SLOTS[3]),
        HostAction::JumpToCue(CUE_SLOTS[4]),
        HostAction::JumpToCue(CUE_SLOTS[5]),
        HostAction::JumpToCue(CUE_SLOTS[6]),
        HostAction::JumpToCue(CUE_SLOTS[7]),
        HostAction::NavLibrary,
        HostAction::NavSearch,
        HostAction::NavNowPlaying,
        HostAction::NavPlugins,
        HostAction::NavSettings,
        HostAction::ToggleQueue,
        HostAction::FocusSearch,
        HostAction::TempoStepUp,
        HostAction::TempoStepDown,
        HostAction::ToggleEffectChain,
        HostAction::ToggleTransportPanel,
    ];

    /// The stable, namespaced identifier persisted to `settings.toml`
    /// and matched by [`HostAction::parse`] (`"host.<category>.<name>"`,
    /// FR-001, FR-002).
    pub const fn id(self) -> &'static str {
        match self {
            HostAction::Play => "host.transport.play",
            HostAction::Pause => "host.transport.pause",
            HostAction::TogglePlayPause => "host.transport.toggle",
            HostAction::Stop => "host.transport.stop",
            HostAction::NextTrack => "host.transport.next",
            HostAction::PreviousTrack => "host.transport.previous",
            HostAction::SeekForwardStep => "host.transport.seek_forward_step",
            HostAction::SeekBackwardStep => "host.transport.seek_backward_step",
            HostAction::VolumeUp => "host.transport.volume_up",
            HostAction::VolumeDown => "host.transport.volume_down",
            HostAction::AddPointMarker => "host.markers.add_point",
            HostAction::SetA => "host.markers.set_a",
            HostAction::SetB => "host.markers.set_b",
            HostAction::NudgeEarlier => "host.markers.nudge_earlier",
            HostAction::NudgeLater => "host.markers.nudge_later",
            HostAction::NudgeEarlierX10 => "host.markers.nudge_earlier_x10",
            HostAction::NudgeLaterX10 => "host.markers.nudge_later_x10",
            HostAction::ClearAllMarkers => "host.markers.clear_all",
            HostAction::ToggleLoop => "host.loop.toggle",
            HostAction::SetCue(slot) => match slot.get() {
                1 => "host.cues.set_1",
                2 => "host.cues.set_2",
                3 => "host.cues.set_3",
                4 => "host.cues.set_4",
                5 => "host.cues.set_5",
                6 => "host.cues.set_6",
                7 => "host.cues.set_7",
                _ => "host.cues.set_8",
            },
            HostAction::JumpToCue(slot) => match slot.get() {
                1 => "host.cues.jump_1",
                2 => "host.cues.jump_2",
                3 => "host.cues.jump_3",
                4 => "host.cues.jump_4",
                5 => "host.cues.jump_5",
                6 => "host.cues.jump_6",
                7 => "host.cues.jump_7",
                _ => "host.cues.jump_8",
            },
            HostAction::NavLibrary => "host.nav.library",
            HostAction::NavSearch => "host.nav.search",
            HostAction::NavNowPlaying => "host.nav.now_playing",
            HostAction::NavPlugins => "host.nav.plugins",
            HostAction::NavSettings => "host.nav.settings",
            HostAction::ToggleQueue => "host.nav.toggle_queue",
            HostAction::FocusSearch => "host.nav.focus_search",
            HostAction::TempoStepUp => "host.effects.tempo_step_up",
            HostAction::TempoStepDown => "host.effects.tempo_step_down",
            HostAction::ToggleEffectChain => "host.nav.toggle_effect_chain",
            HostAction::ToggleTransportPanel => "host.nav.toggle_transport_panel",
        }
    }

    /// The exact inverse of [`HostAction::id`]; `None` for an unknown id
    /// (FR-013's "unknown action id" drop).
    pub fn parse(id: &str) -> Option<HostAction> {
        HostAction::ALL.into_iter().find(|action| action.id() == id)
    }

    /// The Fluent key for this action's display label
    /// (`"action-<category>-<name>"`, contracts/action-registry.md §1).
    pub const fn label_key(self) -> &'static str {
        match self {
            HostAction::Play => "action-transport-play",
            HostAction::Pause => "action-transport-pause",
            HostAction::TogglePlayPause => "action-transport-toggle",
            HostAction::Stop => "action-transport-stop",
            HostAction::NextTrack => "action-transport-next",
            HostAction::PreviousTrack => "action-transport-previous",
            HostAction::SeekForwardStep => "action-transport-seek-forward-step",
            HostAction::SeekBackwardStep => "action-transport-seek-backward-step",
            HostAction::VolumeUp => "action-transport-volume-up",
            HostAction::VolumeDown => "action-transport-volume-down",
            HostAction::AddPointMarker => "action-markers-add-point",
            HostAction::SetA => "action-markers-set-a",
            HostAction::SetB => "action-markers-set-b",
            HostAction::NudgeEarlier => "action-markers-nudge-earlier",
            HostAction::NudgeLater => "action-markers-nudge-later",
            HostAction::NudgeEarlierX10 => "action-markers-nudge-earlier-x10",
            HostAction::NudgeLaterX10 => "action-markers-nudge-later-x10",
            HostAction::ClearAllMarkers => "action-markers-clear-all",
            HostAction::ToggleLoop => "action-loop-toggle",
            HostAction::SetCue(slot) => match slot.get() {
                1 => "action-cues-set-1",
                2 => "action-cues-set-2",
                3 => "action-cues-set-3",
                4 => "action-cues-set-4",
                5 => "action-cues-set-5",
                6 => "action-cues-set-6",
                7 => "action-cues-set-7",
                _ => "action-cues-set-8",
            },
            HostAction::JumpToCue(slot) => match slot.get() {
                1 => "action-cues-jump-1",
                2 => "action-cues-jump-2",
                3 => "action-cues-jump-3",
                4 => "action-cues-jump-4",
                5 => "action-cues-jump-5",
                6 => "action-cues-jump-6",
                7 => "action-cues-jump-7",
                _ => "action-cues-jump-8",
            },
            HostAction::NavLibrary => "action-nav-library",
            HostAction::NavSearch => "action-nav-search",
            HostAction::NavNowPlaying => "action-nav-now-playing",
            HostAction::NavPlugins => "action-nav-plugins",
            HostAction::NavSettings => "action-nav-settings",
            HostAction::ToggleQueue => "action-nav-toggle-queue",
            HostAction::FocusSearch => "action-nav-focus-search",
            HostAction::TempoStepUp => "action-effects-tempo-step-up",
            HostAction::TempoStepDown => "action-effects-tempo-step-down",
            HostAction::ToggleEffectChain => "action-nav-toggle-effect-chain",
            HostAction::ToggleTransportPanel => "action-nav-toggle-transport-panel",
        }
    }

    /// This action's category (data-model.md §1.1's table grouping).
    pub const fn category(self) -> ActionCategory {
        match self {
            HostAction::Play
            | HostAction::Pause
            | HostAction::TogglePlayPause
            | HostAction::Stop
            | HostAction::NextTrack
            | HostAction::PreviousTrack
            | HostAction::SeekForwardStep
            | HostAction::SeekBackwardStep
            | HostAction::VolumeUp
            | HostAction::VolumeDown => ActionCategory::Transport,
            HostAction::AddPointMarker
            | HostAction::SetA
            | HostAction::SetB
            | HostAction::NudgeEarlier
            | HostAction::NudgeLater
            | HostAction::NudgeEarlierX10
            | HostAction::NudgeLaterX10
            | HostAction::ClearAllMarkers => ActionCategory::Markers,
            HostAction::ToggleLoop => ActionCategory::Loop,
            HostAction::SetCue(_) | HostAction::JumpToCue(_) => ActionCategory::Cues,
            HostAction::NavLibrary
            | HostAction::NavSearch
            | HostAction::NavNowPlaying
            | HostAction::NavPlugins
            | HostAction::NavSettings
            | HostAction::ToggleQueue
            | HostAction::FocusSearch
            | HostAction::ToggleEffectChain
            | HostAction::ToggleTransportPanel => ActionCategory::Navigation,
            HostAction::TempoStepUp | HostAction::TempoStepDown => ActionCategory::Effects,
        }
    }
}

/// An action's fixed attributes (DM-14; data-model.md §1.2): category,
/// label, kind, owner, activation scope, repeat behaviour and shipped
/// defaults.
#[derive(Debug)]
pub struct ActionDef {
    pub action: HostAction,
    pub category: ActionCategory,
    pub label_key: &'static str,
    pub kind: ActionKind,
    pub owner: ActionOwner,
    pub scope: Scope,
    pub repeats_while_held: bool,
    pub enabled_by_default: bool,
    pub default_bindings: &'static [&'static str],
}

macro_rules! def {
    ($action:expr, $scope:expr, $repeats:expr, $enabled:expr, $bindings:expr) => {
        ActionDef {
            action: $action,
            category: $action.category(),
            label_key: $action.label_key(),
            kind: ActionKind::Trigger,
            owner: ActionOwner::Host,
            scope: $scope,
            repeats_while_held: $repeats,
            enabled_by_default: $enabled,
            default_bindings: $bindings,
        }
    };
}

/// The shipped default catalog (spec § Default Action Catalog),
/// verbatim: one entry per [`HostAction::ALL`], in the same order.
pub const CATALOG: [ActionDef; 46] = [
    def!(HostAction::Play, Scope::App, false, true, &[]),
    def!(HostAction::Pause, Scope::App, false, true, &[]),
    def!(
        HostAction::TogglePlayPause,
        Scope::App,
        false,
        true,
        &["Space"]
    ),
    def!(HostAction::Stop, Scope::App, false, true, &["Shift+Space"]),
    def!(
        HostAction::NextTrack,
        Scope::App,
        false,
        true,
        &["Primary+Right"]
    ),
    def!(
        HostAction::PreviousTrack,
        Scope::App,
        false,
        true,
        &["Primary+Left"]
    ),
    def!(
        HostAction::SeekForwardStep,
        Scope::App,
        true,
        true,
        &["Primary+Shift+Right"]
    ),
    def!(
        HostAction::SeekBackwardStep,
        Scope::App,
        true,
        true,
        &["Primary+Shift+Left"]
    ),
    def!(
        HostAction::VolumeUp,
        Scope::App,
        true,
        true,
        &["Primary+Up"]
    ),
    def!(
        HostAction::VolumeDown,
        Scope::App,
        true,
        true,
        &["Primary+Down"]
    ),
    def!(
        HostAction::AddPointMarker,
        Scope::NowPlaying,
        false,
        true,
        &["M"]
    ),
    def!(HostAction::SetA, Scope::NowPlaying, false, true, &["I"]),
    def!(HostAction::SetB, Scope::NowPlaying, false, true, &["O"]),
    def!(
        HostAction::NudgeEarlier,
        Scope::MarkerFocused,
        true,
        true,
        &["Left"]
    ),
    def!(
        HostAction::NudgeLater,
        Scope::MarkerFocused,
        true,
        true,
        &["Right"]
    ),
    def!(
        HostAction::NudgeEarlierX10,
        Scope::MarkerFocused,
        true,
        true,
        &["Shift+Left"]
    ),
    def!(
        HostAction::NudgeLaterX10,
        Scope::MarkerFocused,
        true,
        true,
        &["Shift+Right"]
    ),
    def!(
        HostAction::ClearAllMarkers,
        Scope::NowPlaying,
        false,
        true,
        &[]
    ),
    def!(
        HostAction::ToggleLoop,
        Scope::NowPlaying,
        false,
        true,
        &["L"]
    ),
    def!(
        HostAction::SetCue(CUE_SLOTS[0]),
        Scope::NowPlaying,
        false,
        true,
        &["Shift+1"]
    ),
    def!(
        HostAction::SetCue(CUE_SLOTS[1]),
        Scope::NowPlaying,
        false,
        true,
        &["Shift+2"]
    ),
    def!(
        HostAction::SetCue(CUE_SLOTS[2]),
        Scope::NowPlaying,
        false,
        true,
        &["Shift+3"]
    ),
    def!(
        HostAction::SetCue(CUE_SLOTS[3]),
        Scope::NowPlaying,
        false,
        true,
        &["Shift+4"]
    ),
    def!(
        HostAction::SetCue(CUE_SLOTS[4]),
        Scope::NowPlaying,
        false,
        true,
        &["Shift+5"]
    ),
    def!(
        HostAction::SetCue(CUE_SLOTS[5]),
        Scope::NowPlaying,
        false,
        true,
        &["Shift+6"]
    ),
    def!(
        HostAction::SetCue(CUE_SLOTS[6]),
        Scope::NowPlaying,
        false,
        true,
        &["Shift+7"]
    ),
    def!(
        HostAction::SetCue(CUE_SLOTS[7]),
        Scope::NowPlaying,
        false,
        true,
        &["Shift+8"]
    ),
    def!(
        HostAction::JumpToCue(CUE_SLOTS[0]),
        Scope::NowPlaying,
        false,
        true,
        &["1"]
    ),
    def!(
        HostAction::JumpToCue(CUE_SLOTS[1]),
        Scope::NowPlaying,
        false,
        true,
        &["2"]
    ),
    def!(
        HostAction::JumpToCue(CUE_SLOTS[2]),
        Scope::NowPlaying,
        false,
        true,
        &["3"]
    ),
    def!(
        HostAction::JumpToCue(CUE_SLOTS[3]),
        Scope::NowPlaying,
        false,
        true,
        &["4"]
    ),
    def!(
        HostAction::JumpToCue(CUE_SLOTS[4]),
        Scope::NowPlaying,
        false,
        true,
        &["5"]
    ),
    def!(
        HostAction::JumpToCue(CUE_SLOTS[5]),
        Scope::NowPlaying,
        false,
        true,
        &["6"]
    ),
    def!(
        HostAction::JumpToCue(CUE_SLOTS[6]),
        Scope::NowPlaying,
        false,
        true,
        &["7"]
    ),
    def!(
        HostAction::JumpToCue(CUE_SLOTS[7]),
        Scope::NowPlaying,
        false,
        true,
        &["8"]
    ),
    def!(
        HostAction::NavLibrary,
        Scope::App,
        false,
        true,
        &["Primary+1"]
    ),
    def!(
        HostAction::NavSearch,
        Scope::App,
        false,
        true,
        &["Primary+2"]
    ),
    def!(
        HostAction::NavNowPlaying,
        Scope::App,
        false,
        true,
        &["Primary+3"]
    ),
    def!(
        HostAction::NavPlugins,
        Scope::App,
        false,
        true,
        &["Primary+4"]
    ),
    def!(
        HostAction::NavSettings,
        Scope::App,
        false,
        true,
        &["Primary+5"]
    ),
    def!(
        HostAction::ToggleQueue,
        Scope::NowPlaying,
        false,
        true,
        &["Q"]
    ),
    def!(
        HostAction::FocusSearch,
        Scope::App,
        false,
        true,
        &["Primary+F", "Slash"]
    ),
    def!(
        HostAction::TempoStepUp,
        Scope::NowPlaying,
        true,
        true,
        &["Equals", "Plus"]
    ),
    def!(
        HostAction::TempoStepDown,
        Scope::NowPlaying,
        true,
        true,
        &["Minus"]
    ),
    def!(
        HostAction::ToggleEffectChain,
        Scope::NowPlaying,
        false,
        true,
        &["E"]
    ),
    def!(
        HostAction::ToggleTransportPanel,
        Scope::NowPlaying,
        false,
        true,
        &["T"]
    ),
];

/// The catalog entry for `action` (contracts/action-registry.md §1).
pub fn def(action: HostAction) -> &'static ActionDef {
    &CATALOG[HostAction::ALL
        .iter()
        .position(|&a| a == action)
        .unwrap_or(0)]
}

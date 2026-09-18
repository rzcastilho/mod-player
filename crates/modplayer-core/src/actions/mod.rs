// SPDX-License-Identifier: MIT OR Apache-2.0
#![forbid(unsafe_code)]

//! The toolkit-agnostic Action & Binding service: named `HostAction`s
//! (`catalog`), chord parsing/encoding (`chord`), per-user overrides
//! (`keymap`) and the runtime conflict-aware registry (`registry`)
//! (007, data-model.md).

pub mod catalog;
pub mod chord;
pub mod keymap;
pub mod registry;

pub use catalog::{ActionDef, CATALOG, HostAction, def};
pub use chord::{Chord, KEY_NAMES, KeyName, Mods, Platform};
pub use keymap::KeymapOverrides;
pub use registry::{ActionRegistry, ActionRow};

/// One action's category, used to group the shortcut map (FR-006) and to
/// pick its Fluent category label (`ActionCategory::label_key`)
/// (data-model.md §1.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActionCategory {
    Transport,
    Markers,
    Loop,
    Cues,
    Navigation,
    Effects,
}

impl ActionCategory {
    /// Catalog display order (data-model.md §1.1's table order).
    pub const ALL: [ActionCategory; 6] = [
        ActionCategory::Transport,
        ActionCategory::Markers,
        ActionCategory::Loop,
        ActionCategory::Cues,
        ActionCategory::Navigation,
        ActionCategory::Effects,
    ];

    /// The Fluent key for this category's display label
    /// (contracts/action-registry.md §1: `"action-cat-<category>"`).
    pub fn label_key(self) -> &'static str {
        match self {
            ActionCategory::Transport => "action-cat-transport",
            ActionCategory::Markers => "action-cat-markers",
            ActionCategory::Loop => "action-cat-loop",
            ActionCategory::Cues => "action-cat-cues",
            ActionCategory::Navigation => "action-cat-navigation",
            ActionCategory::Effects => "action-cat-effects",
        }
    }
}

/// Whether an action fires once per press (`Trigger`) or streams while
/// held (`Continuous`, DM-14; zero instances ship in this slice —
/// reserved for a MIDI/continuous-control slice, FR-016).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActionKind {
    Trigger,
    Continuous,
}

/// Which subsystem registered an action (DM-14 `owner`). `Host` is the
/// only variant today — no plugin runtime exists (Constitution II,
/// FR-016) — kept as an enum so a later plugin slice adds a variant
/// rather than a breaking-change field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActionOwner {
    Host,
}

/// The view/focus condition under which an action's bindings are live
/// (FR-018). This slice's three scopes nest:
/// `MarkerFocused` ⊂ `NowPlaying` ⊂ `App`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scope {
    App,
    NowPlaying,
    MarkerFocused,
}

impl Scope {
    /// Every scope this slice defines, for exhaustive table-driven tests.
    pub const ALL: [Scope; 3] = [Scope::App, Scope::NowPlaying, Scope::MarkerFocused];

    /// Whether this scope is currently live under `state` (FR-018):
    /// `App` is always live; `NowPlaying` only while the Now Playing view
    /// is shown; `MarkerFocused` only while a marker glyph/row holds
    /// keyboard focus.
    pub fn is_live(self, state: &ScopeState) -> bool {
        match self {
            Scope::App => true,
            Scope::NowPlaying => state.now_playing_shown,
            Scope::MarkerFocused => state.marker_focused,
        }
    }

    /// Whether a binding scoped to `a` and one scoped to `b` could ever
    /// be live at the same moment, and therefore must be checked against
    /// each other for a conflict (FR-010). A lookup table, not a
    /// formula, so a later slice that adds a disjoint scope changes one
    /// row here without touching any call site (plan.md Complexity
    /// Tracking). This slice's three scopes all nest inside one another,
    /// so every pair can coexist.
    pub fn can_coexist(a: Scope, b: Scope) -> bool {
        matches!(
            (a, b),
            (Scope::App, Scope::App)
                | (Scope::App, Scope::NowPlaying)
                | (Scope::App, Scope::MarkerFocused)
                | (Scope::NowPlaying, Scope::App)
                | (Scope::NowPlaying, Scope::NowPlaying)
                | (Scope::NowPlaying, Scope::MarkerFocused)
                | (Scope::MarkerFocused, Scope::App)
                | (Scope::MarkerFocused, Scope::NowPlaying)
                | (Scope::MarkerFocused, Scope::MarkerFocused)
        )
    }
}

/// The view/focus predicates `App` recomputes every frame (research R5),
/// feeding `Scope::is_live`. `marker_focused ⇒ now_playing_shown` is an
/// invariant of how `App` computes this (not enforced here).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ScopeState {
    pub now_playing_shown: bool,
    pub marker_focused: bool,
}

/// `Chord::parse` rejected its input (FR-003, FR-013).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChordParseError {
    /// The empty string.
    Empty,
    /// A modifier token that is not `Primary`/`Shift`/`Alt`.
    UnknownModifier(String),
    /// The same modifier named twice.
    DuplicateModifier,
    /// The final segment is not a known [`KeyName`](chord::KeyName).
    UnknownKey(String),
}

/// `ActionRegistry::add_binding` rejected its input (contracts/
/// action-registry.md §4 rule G5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingError {
    /// The action already holds this exact chord.
    Duplicate,
}

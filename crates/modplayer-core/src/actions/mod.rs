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
pub use registry::{ActionLimit, ActionRegistry, ActionRow, RowCategory, RowLabel};

/// The gateway's own `ActionSource` (011-plugin-ui-contributions,
/// data-model.md §3.1), re-exported so `modplayer-ui`'s dispatcher and
/// `PlaybackController::invoke_plugin_action` never need a direct
/// `modplayer-capability-gateway` dependency just for this one enum.
pub use modplayer_capability_gateway::event::ActionSource;

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

/// Which subsystem registered an action (DM-14 `owner`): the host
/// catalog, or a plugin (011-plugin-ui-contributions, data-model.md
/// §3.1) identified by its session-scoped [`crate::plugins::PluginId`]
/// plus its ownership [`OwnerTier`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActionOwner {
    Host,
    Plugin {
        id: crate::plugins::PluginId,
        tier: OwnerTier,
    },
}

/// An action owner's precedence for conflict resolution (contracts/
/// action-registry-plugins.md R2/G13, FR-011): `Host` always outranks
/// `Bundled`, which always outranks `Community`. Every plugin record is
/// `Source::Bundled` this slice (research), so `Community` is modeled
/// but unreachable in practice until a later slice ships a non-bundled
/// plugin source. Variant order is ascending so `#[derive(Ord)]` gives
/// exactly this precedence (`Host > Bundled > Community`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OwnerTier {
    Community,
    Bundled,
    Host,
}

/// A plugin shortcut action's namespaced identifier (contracts/
/// action-registry-plugins.md R1, FR-010): `"<plugin identifier>.<name>"`,
/// `name` matching the widget-id grammar
/// (`modplayer_capability_gateway::ui::limits::ID_GRAMMAR`).
/// `host.`-namespaced ids are never a valid `PluginActionId` — that
/// namespace is reserved for [`HostAction`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PluginActionId {
    pub plugin: modplayer_capability_gateway::manifest::PluginIdentifier,
    pub name: String,
}

impl PluginActionId {
    /// The registered id: `"<plugin identifier>.<name>"`.
    #[must_use]
    pub fn id(&self) -> String {
        format!("{}.{}", self.plugin, self.name)
    }

    /// The exact inverse of [`PluginActionId::id`] for any value it can
    /// produce: split on the **last** `.` (a plugin identifier may itself
    /// contain dots), `None` if the plugin half fails
    /// [`modplayer_capability_gateway::manifest::PluginIdentifier::parse`],
    /// the name half fails the widget-id grammar, or the plugin half is
    /// (or starts with) `"host"` — that namespace is reserved for
    /// [`HostAction`] (R1) and could otherwise falsely parse (`"host.
    /// markers.add_point"` would split into plugin `"host.markers"`,
    /// name `"add_point"`, both individually grammar-valid).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let (plugin, name) = s.rsplit_once('.')?;
        if plugin == "host" || plugin.starts_with("host.") {
            return None;
        }
        if !modplayer_capability_gateway::ui::limits::matches_id_grammar(name) {
            return None;
        }
        let plugin = modplayer_capability_gateway::manifest::PluginIdentifier::parse(plugin)?;
        Some(Self {
            plugin,
            name: name.to_string(),
        })
    }
}

/// Any action this app can dispatch: a catalog [`HostAction`] or a
/// registered [`PluginActionId`] (contracts/action-registry-plugins.md
/// §1, data-model.md §3.1).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ActionId {
    Host(HostAction),
    Plugin(PluginActionId),
}

impl From<HostAction> for ActionId {
    fn from(action: HostAction) -> Self {
        ActionId::Host(action)
    }
}

impl From<PluginActionId> for ActionId {
    fn from(id: PluginActionId) -> Self {
        ActionId::Plugin(id)
    }
}

/// One `register_action` call's full shape, resolved and ready to enter
/// the registry (contracts/action-registry-plugins.md G10, data-model.md
/// §3.1). `label`/`name` are already resolved against the plugin's own
/// manifest strings (R17) by the caller (`plugins::apply::dispatch`),
/// mirroring `RegisterPanel`'s own resolve-before-validate convention;
/// `name` (the plugin's display name, not in the contract's "indicative"
/// signature) is threaded through so [`ActionRegistry::rows`] can group
/// and sort plugin rows without `actions` depending on `modplayer_
/// capability_gateway::manifest::Manifest`/`PluginRecord` (Constitution
/// III: the registry is a pure, toolkit- and manifest-agnostic shadow
/// state).
#[derive(Debug, Clone, PartialEq)]
pub struct PluginActionDef {
    pub id: PluginActionId,
    pub owner: crate::plugins::PluginId,
    pub tier: OwnerTier,
    /// The owning plugin's resolved display name (manifest `name`, or its
    /// raw identifier when the manifest never validated — mirrors
    /// `plugins::host::plugin_display_name`).
    pub name: String,
    pub label: String,
    pub kind: ActionKind,
    pub repeats_while_held: bool,
    /// `None` when the plugin shipped no default, or 007 FR-007's own
    /// capture would reject the string ([`capture_would_reject`]), or it
    /// fails [`Chord::parse`] — the action still registers, merely unbound
    /// (G10).
    pub default_binding: Option<Chord>,
}

/// A `KeyName` that names a physical modifier key rather than a real key
/// (`KEY_NAMES`'s "emitted only as the `physical_key` half" group) — a
/// chord naming one alone is a lone-modifier binding (mirrors
/// `modplayer-ui::settings::controls::is_modifier_only_key`, the
/// interactive-capture half of this same rule).
fn is_modifier_only_key(name: &str) -> bool {
    matches!(
        name,
        "ShiftLeft"
            | "ShiftRight"
            | "ControlLeft"
            | "ControlRight"
            | "AltLeft"
            | "AltRight"
            | "SuperLeft"
            | "SuperRight"
    )
}

/// Whether `chord` is one 007 FR-007's interactive capture would itself
/// reject (contracts/action-registry-plugins.md G10, FR-010: "lone
/// modifiers, `Tab`/`Shift+Tab`, `Esc`, a macOS `Control` chord") — a
/// plugin's `default_binding` string naming one of these registers the
/// action unbound (with a console warning) rather than refusing the
/// call. The macOS-Control case can never actually reach here: this
/// crate's chord grammar has no token for the physical Control key
/// distinct from `Primary` (Cmd on macOS), so no string a plugin author
/// could write ever encodes it — `modplayer-ui::settings::controls::
/// CaptureRule` rejects it at the raw `egui::Modifiers` level, before a
/// [`Chord`] is ever constructed.
#[must_use]
pub fn capture_would_reject(chord: &Chord) -> bool {
    is_modifier_only_key(chord.key.as_str())
        || chord.key.as_str() == "Tab"
        || (chord.key.as_str() == "Escape" && chord.mods.is_none())
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

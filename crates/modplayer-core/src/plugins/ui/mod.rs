// SPDX-License-Identifier: MIT OR Apache-2.0

//! `PluginUi`: the host-owned per-plugin UI state for the five `ui.*`
//! surfaces (data-model.md §4.1, Constitution III — plugins declare,
//! the host owns and renders). This Foundational skeleton (T030) gives
//! `PluginHost` a lifecycle hook shape that stays stable across every
//! user story; each registry field lands with its own story:
//! `panel.rs`'s `PanelRegistry` (US1 T045/T046), `overlay.rs`'s
//! `OverlayRegistry` (US3 T084) and `settings.rs`'s `SettingsRegistry`
//! (US4 T098, no lifecycle hook of its own — see its own doc comment).

pub mod assets;
pub mod overlay;
pub mod panel;
pub mod settings;
pub mod strings;

use modplayer_capability_gateway::event::UnloadReason;

use super::PluginId;
use overlay::OverlayRegistry;
use panel::PanelRegistry;
use settings::SettingsRegistry;

/// Per-plugin UI state: which panels/overlays/settings page a plugin has
/// registered, owned by `PluginHost` (Constitution III) and applied only
/// from `plugins::apply::dispatch` (research R3).
#[derive(Debug, Default)]
pub struct PluginUi {
    panels: PanelRegistry,
    overlays: OverlayRegistry,
    settings: SettingsRegistry,
}

impl PluginUi {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn panels(&self) -> &PanelRegistry {
        &self.panels
    }

    pub fn panels_mut(&mut self) -> &mut PanelRegistry {
        &mut self.panels
    }

    #[must_use]
    pub fn overlays(&self) -> &OverlayRegistry {
        &self.overlays
    }

    pub fn overlays_mut(&mut self) -> &mut OverlayRegistry {
        &mut self.overlays
    }

    #[must_use]
    pub fn settings(&self) -> &SettingsRegistry {
        &self.settings
    }

    pub fn settings_mut(&mut self) -> &mut SettingsRegistry {
        &mut self.settings
    }

    /// R15 (contracts/ui-panels.md P5): once a plugin becomes `Active`
    /// again after a suspension, any placeholder panel it left behind
    /// must be dropped before it can show as `Live` again — the plugin
    /// is expected to re-register. A no-op here: the actual reset is
    /// deferred to that re-registration itself (`PanelRegistry::
    /// register`'s own `needs_reset` check — see its field doc), which
    /// avoids a same-`tick()` ordering race with the plugin's own request
    /// this method's caller (the host's own `Ready` event) cannot see.
    pub fn on_ready(&mut self, _id: PluginId) {}

    /// R15 (contracts/ui-panels.md P5): `Suspend` keeps this plugin's
    /// panel definitions (the view renders them as a `Placeholder`) but
    /// marks it so its next registration resets first (see
    /// `PanelRegistry::mark_needs_reset`), and clears its overlays (not
    /// yet implemented, US3); `Disable`/`Shutdown` removes its panels
    /// outright.
    pub fn on_stop(&mut self, id: PluginId, reason: UnloadReason) {
        if matches!(reason, UnloadReason::Suspend) {
            self.panels.mark_needs_reset(id);
        } else {
            self.panels.clear(id);
        }
        // O3 (contracts/overlays-settings-notify.md §1.1): overlays clear
        // on *every* stop reason, unlike panels — there is no "suspended
        // placeholder" for an overlay.
        self.overlays.clear(id);
    }

    /// FR-016: every plugin's overlays are cleared on a track change, with
    /// zero plugin code running — panels are untouched by a track change.
    pub fn on_track_changed(&mut self) {
        self.overlays.clear_all();
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! The app shell's left-rail navigation (contracts/ui-surface.md "Main
//! window"): four sections — Library, Now Playing, Plugins, Settings —
//! selectable by pointer or `Ctrl/Cmd+1..4`. Drawing the four nav buttons
//! in this fixed order also fixes their Tab focus order to match (egui's
//! default focus order follows widget-creation order within a frame).
//! `app.rs` owns the `CentralPanel` that renders whichever section is
//! selected; this module only owns the rail and the two placeholder
//! sections that have no dedicated screen yet.

use egui::{Key, Ui};
use modplayer_core::tr;

/// The four navigable sections (contracts/ui-surface.md), in the fixed
/// left-rail display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Library,
    NowPlaying,
    Plugins,
    Settings,
}

/// Every section with its Fluent label key, in nav-rail display order.
const SECTIONS: [(Section, &str); 4] = [
    (Section::Library, "nav-library"),
    (Section::NowPlaying, "nav-now-playing"),
    (Section::Plugins, "nav-plugins"),
    (Section::Settings, "nav-settings"),
];

/// The shell's own UI state: which section is currently selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shell {
    pub section: Section,
}

impl Default for Shell {
    fn default() -> Self {
        Self {
            section: Section::Library,
        }
    }
}

impl Shell {
    /// `Ctrl/Cmd+1..4` jump directly to a section regardless of focus
    /// (contracts/ui-surface.md). Call once per frame before drawing.
    pub fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        ctx.input(|input| {
            if !input.modifiers.command {
                return;
            }
            if input.key_pressed(Key::Num1) {
                self.section = Section::Library;
            } else if input.key_pressed(Key::Num2) {
                self.section = Section::NowPlaying;
            } else if input.key_pressed(Key::Num3) {
                self.section = Section::Plugins;
            } else if input.key_pressed(Key::Num4) {
                self.section = Section::Settings;
            }
        });
    }

    /// Draw the left-rail nav buttons in fixed order, updating `section` on
    /// click. Each button's visible text is also its accessible name
    /// (egui sets both from the same string automatically).
    pub fn nav_rail(&mut self, ui: &mut Ui) {
        for (section, key) in SECTIONS {
            if ui
                .selectable_label(self.section == section, tr(key))
                .clicked()
            {
                self.section = section;
            }
        }
    }
}

/// Library placeholder content (`placeholder-library`) — the real Library
/// screen is a later slice.
pub fn library_placeholder(ui: &mut Ui) {
    ui.label(tr("placeholder-library"));
}

/// Plugins placeholder content (`placeholder-plugins`) — the real Plugins
/// screen is a later slice (Constitution Principle II).
pub fn plugins_placeholder(ui: &mut Ui) {
    ui.label(tr("placeholder-plugins"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::accesskit::Role;
    use egui::{Context, RawInput};
    use modplayer_core::{NotificationCenter, Severity};

    #[test]
    fn default_section_is_library() {
        assert_eq!(Shell::default().section, Section::Library);
    }

    /// Every shell/nav/notification widget exposes an explicit accessible
    /// name (research.md R9's accessibility caveat, FR-022): render the nav
    /// rail, the placeholders, and one notification of each severity, then
    /// walk the AccessKit tree `egui` produces and assert every clickable
    /// (`Role::Button`) node — every nav button, every Dismiss button — has
    /// a non-empty label.
    #[test]
    fn every_shell_and_notification_widget_has_an_accessible_name() {
        let ctx = Context::default();
        ctx.enable_accesskit();

        let mut shell = Shell::default();
        let mut center = NotificationCenter::new();
        center.raise(Severity::Critical, "sample-notification-critical");
        center.raise(Severity::Warning, "sample-notification-warning");
        center.raise(Severity::Info, "sample-notification-info");

        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            shell.nav_rail(ui);
            library_placeholder(ui);
            plugins_placeholder(ui);
            crate::notifications::show(ui, &center);
        });

        let Some(update) = output.platform_output.accesskit_update.take() else {
            panic!("accesskit_update should be populated once enabled");
        };
        // The font atlas texture delta from this synthetic frame is never
        // painted by anything; tell egui we're intentionally discarding it
        // rather than triggering its debug-mode "unapplied deltas" panic.
        output.drop_without_applying_deltas();

        let mut buttons_checked = 0;
        for (_, node) in &update.nodes {
            if node.role() == Role::Button {
                buttons_checked += 1;
                assert!(
                    node.label().is_some_and(|label| !label.is_empty()),
                    "a Button-role node has no accessible name"
                );
            }
        }
        // 4 nav buttons + 3 Dismiss buttons (one per notification raised above).
        assert_eq!(buttons_checked, 7);
    }
}

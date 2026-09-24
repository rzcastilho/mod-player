// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Settings › Playback screen (contracts/ui-surface.md §3, FR-001,
//! T062): a single device-name field, committed on Enter/blur through
//! `PlaybackController::set_device_name`, with an inline "too long" error
//! and an empty field restoring the default name.

use egui::{TextEdit, Ui};
use modplayer_audio_io::OutputBackend;
use modplayer_audio_source::SourceHost;
use modplayer_core::{PlaybackController, tr};

use crate::theme;

/// Owned across frames (mirrors `AboutScreen`/`DeveloperScreen`'s own
/// sub-state) so a draft edit survives repaint and the last-known effective
/// name — used as the field's placeholder, and to avoid recomputing it
/// (`device_name()` shells out to `hostname` when no custom name is set)
/// on every frame — is cached rather than read fresh each frame.
pub struct PlaybackScreen {
    draft: String,
    effective_name: String,
    too_long: bool,
}

impl PlaybackScreen {
    /// Seed the draft and placeholder from `controller`'s current effective
    /// device name (custom or default) once, when the Settings screen is
    /// constructed.
    pub fn new<B: OutputBackend, H: SourceHost>(controller: &PlaybackController<B, H>) -> Self {
        let effective_name = controller.device_name();
        Self {
            draft: effective_name.clone(),
            effective_name,
            too_long: false,
        }
    }
}

/// Draw the device-name field, committing on Enter or losing focus
/// (contracts/ui-surface.md §3: "commits on Enter/blur via
/// `set_device_name`; inline error `setting-device-name-too-long`; empty
/// restores the default and the field shows the default as placeholder").
pub fn show<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    screen: &mut PlaybackScreen,
    focus: Option<&str>,
) {
    let name_label = ui.label(tr("setting-device-name"));
    ui.label(tr("setting-device-name-hint"));

    let response = ui
        .add(TextEdit::singleline(&mut screen.draft).hint_text(screen.effective_name.clone()))
        .labelled_by(name_label.id);
    if focus == Some("playback.device_name") {
        response.request_focus();
    }
    if response.lost_focus() {
        match controller.set_device_name(&screen.draft) {
            Ok(()) => {
                screen.too_long = false;
                screen.effective_name = controller.device_name();
                // An empty/whitespace commit restores the default; show it
                // in the field itself (not just the placeholder) so the
                // effective name is never hidden behind blank text.
                screen.draft = screen.effective_name.clone();
            }
            Err(_) => {
                screen.too_long = true;
            }
        }
    }
    if screen.too_long {
        ui.label(tr("setting-device-name-too-long"));
    }

    show_nudge_step(ui, controller);
}

/// The `[markers] nudge_step_ms` field (006, contracts/ui-markers.md §7):
/// a 1-1000 ms `DragValue`, committed via `set_nudge_step_ms` on every
/// change (the value itself already clamps, so there is no inline error
/// row to show). `update_while_editing(true)` commits on every keystroke
/// rather than only on blur, matching "committing ... on change"
/// (contracts/marker-service.md §5). Returns the `DragValue`'s own
/// `Response` so a caller (this module's own test) can drive it.
fn show_nudge_step<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
) -> egui::Response {
    let label = ui.label(tr("setting-nudge-step"));
    // FR-006, U2: field-description prose, capped at the 72-character
    // measure (research R17).
    ui.scope(|ui| {
        ui.set_max_width(ui.available_width().min(theme::body_measure(ui.ctx())));
        ui.label(tr("setting-nudge-step-desc"));
    });
    let mut value = i64::from(controller.nudge_step_ms());
    let response = ui
        .add(
            egui::DragValue::new(&mut value)
                .range(1..=1_000)
                .suffix(" ms")
                .update_while_editing(true),
        )
        .labelled_by(label.id);
    if response.changed() {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let ms = value.clamp(1, 1_000) as u16;
        controller.set_nudge_step_ms(ms);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use modplayer_audio_io::FakeBackend;
    use modplayer_core::SettingsStore;

    fn fresh_store() -> SettingsStore {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-ui-playback-settings-test-{}-{}",
            std::process::id(),
            unique
        ));
        let _ = std::fs::create_dir_all(&dir);
        SettingsStore::with_path(dir.join("settings.toml"))
    }

    #[test]
    fn new_seeds_the_draft_from_the_current_effective_name() {
        let controller = PlaybackController::new(
            FakeBackend::new(vec![]),
            modplayer_audio_source_synthetic::SyntheticHost::new(44_100),
            fresh_store(),
        );
        let screen = PlaybackScreen::new(&controller);
        assert_eq!(screen.draft, controller.device_name());
        assert_eq!(screen.effective_name, controller.device_name());
        assert!(!screen.too_long);
    }

    fn default_input() -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400.0, 200.0),
            )),
            ..Default::default()
        }
    }

    /// `setting-nudge-step`'s `DragValue` commits via `set_nudge_step_ms`
    /// (006, contracts/ui-markers.md §7): typing a new value into it
    /// updates the controller's shadow state the same frame
    /// (`update_while_editing(true)`).
    #[test]
    fn nudge_step_drag_value_commits() {
        let mut controller = PlaybackController::new(
            FakeBackend::new(vec![]),
            modplayer_audio_source_synthetic::SyntheticHost::new(44_100),
            fresh_store(),
        );
        assert_eq!(controller.nudge_step_ms(), 10, "sanity: default nudge step");

        let ctx = egui::Context::default();

        // Frame 1: request focus onto the DragValue so frame 2 renders it
        // already in keyboard-edit mode (egui's own documented behaviour:
        // a `DragValue` that gains focus is immediately shown in edit
        // mode, never button mode, for one frame).
        let output = ctx.run_ui(default_input(), |ui| {
            show_nudge_step(ui, &mut controller).request_focus();
        });
        output.drop_without_applying_deltas();

        // Frame 2: clear the existing "10" (egui's own "select all text on
        // gained focus" isn't reliable in a headless test's `run_ui`, so
        // clear it explicitly) and type "500".
        let mut input = default_input();
        input.events.push(egui::Event::Key {
            key: egui::Key::Backspace,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        input.events.push(egui::Event::Key {
            key: egui::Key::Backspace,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        input.events.push(egui::Event::Text("500".to_string()));
        let output = ctx.run_ui(input, |ui| {
            show_nudge_step(ui, &mut controller);
        });
        output.drop_without_applying_deltas();

        assert_eq!(controller.nudge_step_ms(), 500);
    }
}

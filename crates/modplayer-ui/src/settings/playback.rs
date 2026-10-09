// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Settings › Playback screen (contracts/ui-surface.md §3, FR-001,
//! T062): a single device-name field, committed on Enter/blur through
//! `PlaybackController::set_device_name`, with an inline "too long" error
//! and an empty field restoring the default name.

use egui::{TextEdit, Ui};
use modplayer_audio_io::OutputBackend;
use modplayer_audio_source::SourceHost;
use modplayer_core::settings::NUDGE_STEP_MS_RANGE;
use modplayer_core::{AudioSettings, PlaybackController, tr};

use crate::settings::field::{self, FieldSpec, ResetState, Unit};
use crate::theme::tokens::space;
use crate::widgets::controls::panel_card;

/// Owned across frames (mirrors `AboutScreen`/`DeveloperScreen`'s own
/// sub-state) so a draft edit survives repaint and the last-known effective
/// name — used as the field's placeholder, and to avoid recomputing it
/// (`device_name()` shells out to `hostname` when no custom name is set)
/// on every frame — is cached rather than read fresh each frame.
pub struct PlaybackScreen {
    draft: String,
    effective_name: String,
    too_long: bool,
    /// A custom name is stored (`[playback] device_name` is `Some`); the
    /// Reset offer keys off this, not the effective name (research R4).
    custom_name: bool,
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
            custom_name: controller
                .settings_store()
                .load()
                .settings
                .device_name
                .is_some(),
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
    panel_card(ui, &tr("settings-group-connect-device"), |ui| {
        let mut spec = FieldSpec::new("playback.device_name", tr("setting-device-name"));
        spec.help = Some(tr("setting-device-name-hint"));
        spec.reset = if screen.custom_name {
            ResetState::Offered
        } else {
            ResetState::Hidden
        };
        let output = field::row(ui, &spec, |ui| {
            ui.add(TextEdit::singleline(&mut screen.draft).hint_text(screen.effective_name.clone()))
        });
        if let Some(id) = output.label_id {
            output.control.clone().labelled_by(id);
        }
        let response = output.control;
        if focus == Some("playback.device_name") {
            response.request_focus();
        }
        if output.reset_clicked {
            screen.draft.clear();
        }
        if response.lost_focus() || output.reset_clicked {
            match controller.set_device_name(&screen.draft) {
                Ok(()) => {
                    screen.too_long = false;
                    screen.custom_name = !screen.draft.trim().is_empty();
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
    });
    ui.add_space(space::LG);

    panel_card(ui, &tr("settings-group-markers"), |ui| {
        let response = show_nudge_step(ui, controller);
        if focus == Some("markers.nudge_step_ms") {
            response.request_focus();
        }
    });
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
    let mut value = i64::from(controller.nudge_step_ms());
    let mut spec = FieldSpec::new("markers.nudge_step_ms", tr("setting-nudge-step"));
    spec.reset = ResetState::compute(
        &controller.nudge_step_ms(),
        &AudioSettings::default().nudge_step_ms,
    );
    spec.help = Some(tr("setting-nudge-step-desc"));
    let (min, max) = (*NUDGE_STEP_MS_RANGE.start(), *NUDGE_STEP_MS_RANGE.end());
    spec.range = Some(field::format_range(
        Unit::Ms,
        f64::from(min),
        f64::from(max),
    ));
    let output = field::row(ui, &spec, |ui| {
        ui.add(
            egui::DragValue::new(&mut value)
                .range(i64::from(min)..=i64::from(max))
                .custom_formatter(|n, _| field::format_value(Unit::Ms, n))
                .custom_parser(|text| field::parse_value(Unit::Ms, text))
                .update_while_editing(true),
        )
    });
    if let Some(id) = output.label_id {
        output.control.clone().labelled_by(id);
    }
    let reset_clicked = output.reset_clicked;
    let response = output.control;
    if reset_clicked {
        controller.set_nudge_step_ms(AudioSettings::default().nudge_step_ms);
    } else if response.changed() {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let ms = value.clamp(i64::from(min), i64::from(max)) as u16;
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

        // Frame 2: clear the existing "10 ms" (egui's own "select all text on
        // gained focus" isn't reliable in a headless test's `run_ui`, so
        // clear it explicitly) and type "500".
        let mut input = default_input();
        // The edit buffer is the unit-bearing text ("10 ms", five chars).
        for _ in 0.."10 ms".len() {
            input.events.push(egui::Event::Key {
                key: egui::Key::Backspace,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            });
        }
        input.events.push(egui::Event::Text("500".to_string()));
        let output = ctx.run_ui(input, |ui| {
            show_nudge_step(ui, &mut controller);
        });
        output.drop_without_applying_deltas();

        assert_eq!(controller.nudge_step_ms(), 500);
    }
}

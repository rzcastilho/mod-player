// SPDX-License-Identifier: MIT OR Apache-2.0

//! Settings › Plugins (011-plugin-ui-contributions US4 T102,
//! contracts/overlays-settings-notify.md §2 "S2"-"S4"): the 009 plugin-
//! management placeholder content stays (`placeholder-settings-category`,
//! T104's own doc note), and below it a "Plugin settings" list — one row
//! per `Active` plugin that has ever registered a settings page
//! (`PlaybackController::plugin_settings_views`, sorted by name) — opening
//! a per-plugin sub-page with one field renderer per `FieldKind` (S3).
//!
//! `focus` is `Some((plugin, field_id))` the one frame a Settings-search
//! hit (`settings_registry::search_plugin_settings`, `settings/mod.rs`)
//! asks to land here: it forces that plugin's sub-page open and claims
//! keyboard focus on that field, mirroring every other category screen's
//! own `focus: Option<&str>` convention (`audio.rs`/`playback.rs`).
//!
//! Every field's current value comes from the plugin's own live
//! `SettingsPage::values` (never a separate UI-owned copy) so a search
//! re-open and the plugin's own `settings_changed` echo never disagree
//! (S7). A `number`/`string` field keeps a UI-only draft in `egui` memory
//! while it is actively being edited (S4: "on commit"), applying through
//! [`PlaybackController::plugin_settings_edit`] only once that edit ends
//! (`lost_focus`, or `drag_stopped` for the slider half of "Slider with
//! `DragValue`" — a mouse drag never focuses a `Slider`, egui's own
//! `interaction.rs`, so `lost_focus` alone would never fire for a pure
//! pointer drag). `boolean`/`choice` apply immediately on change (S4),
//! having no draft to defer.

use egui::{ComboBox, Id, Slider, TextEdit, Ui};
use modplayer_audio_io::OutputBackend;
use modplayer_audio_source::SourceHost;
use modplayer_capability_gateway::ui::limits::MAX_STRING_FIELD_CHARS;
use modplayer_capability_gateway::ui::{FieldKind, ListItem, SettingsField};
use modplayer_core::{PlaybackController, PluginId, tr};

use crate::theme;
use crate::widgets::controls::{SwitchKind, switch};

/// UI-only navigation state: which plugin's sub-page (if any) is open.
/// Field values themselves are never cached here — they are read fresh
/// from `PluginSettingsView` every frame (S7).
#[derive(Debug, Default)]
pub struct PluginsScreen {
    open: Option<PluginId>,
}

impl PluginsScreen {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

/// Draw the Plugins category screen: the 009 placeholder plus the
/// settings-page list, or (once a page is opened, or a search hit names
/// one) that plugin's own sub-page.
pub fn show<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    screen: &mut PluginsScreen,
    focus: Option<(PluginId, &str)>,
) {
    if let Some((plugin, _)) = focus {
        screen.open = Some(plugin);
    }

    let views = controller.plugin_settings_views();

    if let Some(plugin) = screen.open {
        if let Some(view) = views.iter().find(|v| v.plugin == plugin) {
            if ui.button(tr("settings-plugins-back")).clicked() {
                screen.open = None;
            }
            // 014-design-tokens-and-type-scale (US2, T033, data-model.md §6
            // "settings groups" -> `theme::section_label`), with the
            // accesskit label pinned back to the exact `view.name` (mirrors
            // `settings::controls::section_heading`/`markers::panel`, T030)
            // so the accessible name doesn't pick up the painted uppercase
            // text (FR-019).
            let heading = ui.label(theme::section_label(&view.name));
            ui.ctx().accesskit_node_builder(heading.id, |b| {
                b.set_label(view.name.clone());
            });
            let field_focus = focus.and_then(|(p, f)| (p == plugin).then_some(f));
            for field in &view.page.fields {
                let value = view.page.values.get(field.id.as_str());
                show_field(ui, controller, plugin, field, value, field_focus);
            }
            return;
        }
        // The plugin closed/hid its page (or went inactive) since this was
        // opened — fall through to the list below instead of a dead page.
        screen.open = None;
    }

    ui.label(tr("placeholder-settings-category"));
    theme::divider(ui);
    ui.label(tr("settings-plugins-pages"));
    if views.is_empty() {
        ui.label(tr("settings-plugins-none"));
        return;
    }
    for view in &views {
        if ui.selectable_label(false, view.name.clone()).clicked() {
            screen.open = Some(view.plugin);
        }
    }
}

/// Attach `field`'s own `description` (if any) as both a visible tooltip
/// and the AccessKit description (S3) — the same `accesskit_node_builder`
/// pattern `waveform/mod.rs` already uses for the seek slider's windowed
/// description.
fn finish_field(ui: &Ui, response: egui::Response, field: &SettingsField) -> egui::Response {
    let Some(description) = &field.description else {
        return response;
    };
    let response = response.on_hover_text(description.clone());
    ui.ctx().accesskit_node_builder(response.id, |builder| {
        builder.set_description(description.clone());
    });
    response
}

fn show_field<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    plugin: PluginId,
    field: &SettingsField,
    value: Option<&serde_json::Value>,
    focus: Option<&str>,
) {
    let focus_matches = focus == Some(field.id.as_str());
    match &field.kind {
        FieldKind::Boolean => show_boolean(ui, controller, plugin, field, value, focus_matches),
        FieldKind::Number { min, max, step } => {
            show_number(
                ui,
                controller,
                plugin,
                field,
                *min,
                *max,
                *step,
                value,
                focus_matches,
            );
        }
        FieldKind::String => show_string(ui, controller, plugin, field, value, focus_matches),
        FieldKind::Choice { options } => {
            show_choice(ui, controller, plugin, field, options, value, focus_matches);
        }
    }
}

/// `boolean` → the switch (015-control-variants, data-model.md §8:
/// `SwitchKind::Checkbox`), applied on change (S3/S4).
fn show_boolean<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    plugin: PluginId,
    field: &SettingsField,
    value: Option<&serde_json::Value>,
    focus_matches: bool,
) {
    let mut checked = value
        .and_then(serde_json::Value::as_bool)
        .unwrap_or_else(|| field.default.as_bool().unwrap_or(false));
    let response = switch(ui, SwitchKind::Checkbox, &mut checked, &field.label);
    let response = finish_field(ui, response, field);
    if focus_matches {
        response.request_focus();
    }
    if response.changed() {
        controller.plugin_settings_edit(
            plugin,
            field.id.as_str(),
            serde_json::Value::from(checked),
        );
    }
}

fn number_mem_id(plugin: PluginId, field: &SettingsField) -> Id {
    Id::new(("settings-plugin-number", plugin.0, field.id.as_str()))
}

/// `number` → `Slider` with its own embedded `DragValue` readout (`egui`'s
/// default `show_value(true)`), clamped to `[min, max]` (S3). A draft
/// value persists in `egui` memory across frames while the widget is
/// being dragged or holds keyboard focus; every other frame it re-syncs
/// from the field's own live value (so an external change — another
/// viewer's edit, a re-registration — is never fought). Applies on
/// `lost_focus` (keyboard edit ends) or `drag_stopped` (S4's "commit",
/// covering the pointer-drag half — a `Slider` drag never itself claims
/// keyboard focus, so `lost_focus` alone would never fire for it).
#[allow(clippy::too_many_arguments)]
fn show_number<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    plugin: PluginId,
    field: &SettingsField,
    min: f64,
    max: f64,
    step: f64,
    value: Option<&serde_json::Value>,
    focus_matches: bool,
) {
    let current = value
        .and_then(serde_json::Value::as_f64)
        .unwrap_or_else(|| field.default.as_f64().unwrap_or(min));
    let mem_id = number_mem_id(plugin, field);
    let mut draft: f64 = ui.memory(|m| m.data.get_temp(mem_id)).unwrap_or(current);

    let response = ui.add(
        Slider::new(&mut draft, min..=max)
            .step_by(step.max(f64::EPSILON))
            .text(field.label.clone()),
    );
    let response = finish_field(ui, response, field);
    if focus_matches {
        response.request_focus();
    }

    let commit_value = draft;
    if !response.has_focus() && !response.dragged() {
        // Not actively being edited this frame — re-sync from the live
        // value (a plain no-op once this very edit's own commit below has
        // landed, since the next frame's `current` already reflects it).
        draft = current;
    }
    ui.memory_mut(|m| m.data.insert_temp(mem_id, draft));

    if response.lost_focus() || response.drag_stopped() {
        controller.plugin_settings_edit(
            plugin,
            field.id.as_str(),
            serde_json::Value::from(commit_value),
        );
    }
}

fn string_mem_id(plugin: PluginId, field: &SettingsField) -> Id {
    Id::new(("settings-plugin-string", plugin.0, field.id.as_str()))
}

/// `string` → single-line `TextEdit` (`char_limit(1024)`), applied on
/// commit (`Enter` or `lost_focus`, S3/S4) — mirrors `playback.rs`'s own
/// device-name field.
fn show_string<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    plugin: PluginId,
    field: &SettingsField,
    value: Option<&serde_json::Value>,
    focus_matches: bool,
) {
    let current = value
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| field.default.as_str().unwrap_or_default().to_string());
    let mem_id = string_mem_id(plugin, field);
    let mut draft: String = ui
        .memory(|m| m.data.get_temp(mem_id))
        .unwrap_or_else(|| current.clone());

    let name_label = ui.label(field.label.clone());
    let response = ui
        .add(TextEdit::singleline(&mut draft).char_limit(MAX_STRING_FIELD_CHARS))
        .labelled_by(name_label.id);
    let response = finish_field(ui, response, field);
    if focus_matches {
        response.request_focus();
    }

    let commit_value = draft.clone();
    if !response.has_focus() {
        draft = current;
    }
    ui.memory_mut(|m| m.data.insert_temp(mem_id, draft));

    if response.lost_focus() {
        controller.plugin_settings_edit(
            plugin,
            field.id.as_str(),
            serde_json::Value::from(commit_value),
        );
    }
}

/// `choice` → `ComboBox`, applied on change (S3/S4). Unlike `Slider`/
/// `Checkbox`, `egui`'s own `ComboBox` names itself after
/// `.selected_text(..)` (the *selected option's* text), not a field
/// label passed in separately — so, like `string`'s `TextEdit`, its
/// accessible name comes from a preceding `ui.label` via `.labelled_by`
/// rather than from the widget's own text.
#[allow(clippy::too_many_arguments)]
fn show_choice<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    plugin: PluginId,
    field: &SettingsField,
    options: &[ListItem],
    value: Option<&serde_json::Value>,
    focus_matches: bool,
) {
    let current: Option<String> = value
        .and_then(|v| v.as_str())
        .or_else(|| field.default.as_str())
        .map(str::to_string);
    let selected_label = current
        .as_deref()
        .and_then(|id| options.iter().find(|option| option.id.as_str() == id))
        .map(|option| option.label.clone())
        .unwrap_or_default();

    let name_label = ui.label(field.label.clone());
    let mut newly_selected: Option<String> = None;
    let combo_response =
        ComboBox::from_id_salt(("settings-plugin-choice", plugin.0, field.id.as_str()))
            .selected_text(selected_label)
            .show_ui(ui, |ui| {
                for option in options {
                    let is_selected = current.as_deref() == Some(option.id.as_str());
                    if ui
                        .selectable_label(is_selected, option.label.clone())
                        .clicked()
                        && !is_selected
                    {
                        newly_selected = Some(option.id.as_str().to_string());
                    }
                }
            })
            .response
            .labelled_by(name_label.id);
    let combo_response = finish_field(ui, combo_response, field);
    if focus_matches {
        combo_response.request_focus();
    }
    if let Some(id) = newly_selected {
        controller.plugin_settings_edit(plugin, field.id.as_str(), serde_json::Value::from(id));
    }
}

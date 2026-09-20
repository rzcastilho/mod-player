// SPDX-License-Identifier: MIT OR Apache-2.0

//! Settings › Controls (007, US2, T057-T060; data-model.md §4.3,
//! contracts/ui-actions.md §4/§5, research R11): the whole `HostAction`
//! catalog grouped by category with a filter box, per-binding chips a
//! user can remove, a focus-locked capture control for "Add binding",
//! per-action "Reset to default" and a page-level two-step "Reset all to
//! defaults" (US3's `⚠`/conflict-partner surfacing and US4's restart
//! proof are later slices — the registry's conflict engine already runs
//! underneath every mutation here, Phase 2).

use egui::{
    Button, Event, EventFilter, Id, Key, Modifiers, RichText, TextEdit, Ui, WidgetInfo, WidgetType,
};
use modplayer_audio_io::OutputBackend;
use modplayer_audio_source::SourceHost;
use modplayer_core::actions::{
    ActionCategory, ActionId, ActionKind, Chord, Platform, RowCategory, RowLabel,
};
use modplayer_core::{PlaybackController, tr, tr_args};

use crate::actions::{self, Claim, key_name_from_egui, mods_from_egui};

/// UI-only state for the Controls screen (data-model.md §4.3): the live
/// filter text, which action (if any) is mid-capture, the last inline
/// capture rejection, and the page-level "Reset all" two-step confirm.
/// Transient bookkeeping for *which frame* a capture/confirm control
/// first requested focus lives in `egui::Context` memory instead (mirrors
/// `settings/account.rs`'s sign-out modal `modal_focus_pending_id`), so
/// this struct stays exactly the four fields the contract names.
/// `capture: Option<ActionId>` (011-plugin-ui-contributions, widened from
/// `HostAction`, contracts/action-registry-plugins.md FR-010): a plugin
/// action's row opens the very same capture control.
#[derive(Debug, Clone, Default)]
pub struct ControlsScreen {
    pub filter: String,
    pub capture: Option<ActionId>,
    pub capture_error: Option<&'static str>,
    pub reset_all_confirm: bool,
}

/// Why `CaptureRule::check` rejected a candidate (data-model.md §4.3;
/// contracts/ui-actions.md §5) — each variant's Fluent key is the inline
/// message shown next to the still-open capture control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureReject {
    /// The event carried modifiers but no real key (either no
    /// `Event::Key` fired at all — egui-winit never emits one for a bare
    /// modifier press — or, the synthetic case this is actually reachable
    /// for, the resolved key name is itself a physical-modifier name like
    /// `ShiftLeft`).
    LoneModifier,
    /// `Tab`/`Shift+Tab` — reserved for focus navigation.
    TabReserved,
    /// macOS's physical Control key, unbindable in this slice.
    MacControl,
    /// The same chord this action already holds.
    Duplicate,
}

impl CaptureReject {
    /// The Fluent key for this rejection's inline message.
    pub fn message_key(self) -> &'static str {
        match self {
            CaptureReject::LoneModifier => "controls-reject-modifier-only",
            CaptureReject::TabReserved => "controls-reject-tab",
            CaptureReject::MacControl => "controls-reject-mac-control",
            CaptureReject::Duplicate => "controls-reject-duplicate",
        }
    }
}

/// FR-007's capture-mode validation, applied to one pressed `Event::Key`.
pub struct CaptureRule;

impl CaptureRule {
    /// `raw` MUST be a `Event::Key { pressed: true, .. }`; `existing` is
    /// the action's current effective bindings (the duplicate check).
    /// Returns the canonical [`Chord`] to bind, or why the candidate was
    /// rejected.
    pub fn check(raw: &Event, is_mac: bool, existing: &[Chord]) -> Result<Chord, CaptureReject> {
        let Event::Key {
            key,
            physical_key,
            modifiers,
            pressed: true,
            ..
        } = raw
        else {
            return Err(CaptureReject::LoneModifier);
        };

        if *key == Key::Tab {
            return Err(CaptureReject::TabReserved);
        }
        if is_mac && modifiers.ctrl {
            return Err(CaptureReject::MacControl);
        }

        let Some(chord) = canonical_capture_chord(*key, *physical_key, *modifiers, is_mac) else {
            return Err(CaptureReject::LoneModifier);
        };
        if is_modifier_only_key(chord.key.as_str()) {
            return Err(CaptureReject::LoneModifier);
        }
        if existing.contains(&chord) {
            return Err(CaptureReject::Duplicate);
        }
        Ok(chord)
    }
}

/// A `KeyName` that names a physical modifier key rather than a real key
/// (`KEY_NAMES`'s "emitted only as the `physical_key` half" group) — a
/// press of one alone, with no other key, is a lone modifier
/// (`CaptureReject::LoneModifier`'s synthetic case).
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

/// research R3's capture canonicalisation (data-model.md §4.3, design
/// note 4): the physical chord when Shift is held and the layout consumed
/// it to produce a different logical key, else the logical chord —
/// exactly `dispatch`'s own pass-1/pass-2 split, reused via
/// `crate::actions`'s `pub(crate)` conversions.
fn canonical_capture_chord(
    key: egui::Key,
    physical_key: Option<egui::Key>,
    modifiers: Modifiers,
    is_mac: bool,
) -> Option<Chord> {
    let mods = mods_from_egui(modifiers, is_mac)?;
    let logical = key_name_from_egui(key)?;
    let shift_consumed = physical_key.is_some_and(|physical| physical != key);
    if mods.shift && shift_consumed {
        let physical_name = physical_key.and_then(key_name_from_egui)?;
        return Some(Chord::new(mods, physical_name));
    }
    Some(Chord::new(mods, logical))
}

/// Which heading group a row falls under (011-plugin-ui-contributions,
/// contracts/action-registry-plugins.md G15): a host category, or a
/// named owning plugin.
#[derive(Debug, Clone, PartialEq, Eq)]
enum RowGroup {
    Host(ActionCategory),
    Plugin(String),
}

/// One catalog row's data, snapshotted from `controller.actions().rows()`
/// before any widget can mutate the registry through a button in the
/// same frame (`ActionRow` borrows the registry; holding that borrow
/// across a `controller.add_binding`/`remove_binding` call would not
/// compile). Widened from the 007 `HostAction`-only shape
/// (011-plugin-ui-contributions, contracts/action-registry-plugins.md
/// G15) so a plugin's own actions render through this exact same row.
struct RowSnapshot {
    id: ActionId,
    label: String,
    kind: ActionKind,
    enabled: bool,
    bindings: Vec<Chord>,
    conflicts: Vec<Chord>,
    group: RowGroup,
}

fn snapshot_rows<B: OutputBackend, H: SourceHost>(
    controller: &PlaybackController<B, H>,
) -> Vec<RowSnapshot> {
    controller
        .actions()
        .rows()
        .map(|row| {
            let label = match row.label {
                RowLabel::Fluent(key) => tr(key),
                RowLabel::Literal(s) => s,
            };
            let group = match row.category {
                RowCategory::Host(category) => RowGroup::Host(category),
                RowCategory::Plugin { name } => RowGroup::Plugin(name),
            };
            RowSnapshot {
                id: row.id,
                label,
                kind: row.kind,
                enabled: row.enabled,
                bindings: row.bindings.to_vec(),
                conflicts: row.conflicts,
                group,
            }
        })
        .collect()
}

/// `id`'s display label, resolved the same way [`snapshot_rows`] resolves
/// a row's own — used to name a *conflict partner* (`show_chip`), which
/// is looked up fresh against the live registry rather than carried in
/// `RowSnapshot` (a chip's conflict partner is a different row, not
/// itself).
fn action_label<B: OutputBackend, H: SourceHost>(
    controller: &PlaybackController<B, H>,
    id: &ActionId,
) -> String {
    match id {
        ActionId::Host(action) => tr(action.label_key()),
        ActionId::Plugin(plugin_id) => controller
            .actions()
            .plugin_action_def(plugin_id)
            .map(|def| def.label.clone())
            .unwrap_or_default(),
    }
}

/// The tier-aware "conflicts with …" message (contracts/action-registry-
/// plugins.md G15: "conflict text names the partner's tier"):
/// `controls-conflict-with-host` when `partner` is the host catalog,
/// `controls-conflict-with-plugin` (naming the owning plugin) otherwise.
fn conflict_message<B: OutputBackend, H: SourceHost>(
    controller: &PlaybackController<B, H>,
    partner: &ActionId,
) -> String {
    let other = action_label(controller, partner);
    match partner {
        ActionId::Host(_) => tr_args("controls-conflict-with-host", &[("other", other)]),
        ActionId::Plugin(plugin_id) => {
            let plugin = controller
                .actions()
                .plugin_action_def(plugin_id)
                .map(|def| def.name.clone())
                .unwrap_or_default();
            tr_args(
                "controls-conflict-with-plugin",
                &[("plugin", plugin), ("other", other)],
            )
        }
    }
}

/// Draw the Controls screen: the filter box, "Reset all to defaults",
/// then every category with at least one matching row (contracts/
/// ui-actions.md §4). `focus == Some("controls.keybindings")` — a
/// settings-search hit on this page's own descriptor — focuses the
/// filter box.
pub fn show<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    screen: &mut ControlsScreen,
    focus: Option<&str>,
) {
    let platform = if ui.ctx().os().is_mac() {
        Platform::Mac
    } else {
        Platform::Other
    };
    let rows = snapshot_rows(controller);

    ui.heading(tr("controls-heading"));

    ui.horizontal(|ui| {
        let filter_label = ui.label(tr("controls-filter"));
        let response = ui
            .add(TextEdit::singleline(&mut screen.filter).id(filter_box_id()))
            .labelled_by(filter_label.id);
        actions::register_claim(ui.ctx(), response.id, Claim::TextLike);
        if focus == Some("controls.keybindings") {
            response.request_focus();
        }
    });

    show_reset_all(ui, controller, screen);
    ui.separator();

    let query = screen.filter.to_lowercase();
    let mut any_match = false;
    for category in ActionCategory::ALL {
        let category_matches = tr(category.label_key()).to_lowercase().contains(&query);
        let category_rows: Vec<&RowSnapshot> = rows
            .iter()
            .filter(|row| row.group == RowGroup::Host(category))
            .filter(|row| category_matches || row.label.to_lowercase().contains(&query))
            .collect();
        if category_rows.is_empty() {
            continue;
        }
        any_match = true;
        ui.heading(tr(category.label_key()));
        for row in category_rows {
            show_row(ui, controller, screen, row, platform);
        }
    }

    // One heading per owning plugin, in `rows()`'s own order (already
    // sorted by display name, contracts/action-registry-plugins.md G15) —
    // `dedup`-free grouping by walking the already-grouped snapshot once.
    let mut seen_plugins: Vec<&str> = Vec::new();
    for row in &rows {
        let RowGroup::Plugin(name) = &row.group else {
            continue;
        };
        if seen_plugins.contains(&name.as_str()) {
            continue;
        }
        seen_plugins.push(name.as_str());

        let heading_matches = name.to_lowercase().contains(&query);
        let plugin_rows: Vec<&RowSnapshot> = rows
            .iter()
            .filter(|r| matches!(&r.group, RowGroup::Plugin(n) if n == name))
            .filter(|r| heading_matches || r.label.to_lowercase().contains(&query))
            .collect();
        if plugin_rows.is_empty() {
            continue;
        }
        any_match = true;
        ui.heading(tr_args(
            "controls-plugin-group",
            &[("plugin", name.clone())],
        ));
        for row in plugin_rows {
            show_row(ui, controller, screen, row, platform);
        }
    }

    if !any_match {
        ui.label(tr("controls-no-match"));
    }
}

fn filter_box_id() -> Id {
    Id::new("controls-filter-box")
}

/// One action's row: label (+ "(inactive)" when disabled/the owning
/// plugin is not Active, de-emphasized — contracts/action-registry-
/// plugins.md G15 "a row whose plugin is not Active renders greyed"),
/// kind, one chip per binding, "Add binding" (or the open capture
/// control), and "Reset to default" (contracts/ui-actions.md §4).
fn show_row<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    screen: &mut ControlsScreen,
    row: &RowSnapshot,
    platform: Platform,
) {
    ui.horizontal_wrapped(|ui| {
        let label = row.label.clone();
        let label_text = if row.enabled {
            label.clone()
        } else {
            format!("{label} {}", tr("controls-inactive"))
        };
        let text = RichText::new(label_text);
        let text = if row.enabled {
            text
        } else {
            text.color(ui.visuals().weak_text_color())
        };
        ui.label(text);

        let kind_key = match row.kind {
            ActionKind::Trigger => "controls-kind-trigger",
            ActionKind::Continuous => "controls-kind-continuous",
        };
        ui.label(tr(kind_key));

        for &chord in &row.bindings {
            let conflicting = row.conflicts.contains(&chord);
            show_chip(ui, controller, row.id.clone(), chord, platform, conflicting);
        }

        if screen.capture.as_ref() == Some(&row.id) {
            show_capture_control(ui, controller, screen, row.id.clone(), &row.bindings);
        } else if ui.button(tr("controls-add-binding")).clicked() {
            open_capture(ui, screen, row.id.clone());
        }

        let reset_label = tr_args("controls-reset-action", &[("action", label)]);
        if ui.button(reset_label).clicked() {
            controller.reset_binding(row.id.clone());
        }
    });
}

/// One binding chip: a non-interactive label showing the platform display
/// string (accessible name `controls-binding-chip { $binding }`), a `⚠` +
/// a tier-aware "conflicts with …" note ([`conflict_message`]) when
/// `chord` is currently flagged as conflicting (`conflicting`, from the
/// row's own snapshotted `ActionRow::conflicts` — US3, contracts/
/// ui-actions.md §4, contracts/action-registry-plugins.md G15), and a `×`
/// button that removes it immediately (FR-008).
fn show_chip<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    action: ActionId,
    chord: Chord,
    platform: Platform,
    conflicting: bool,
) {
    let conflict_partner = conflicting
        .then(|| controller.actions().conflict_partner(action.clone(), chord))
        .flatten();

    ui.horizontal(|ui| {
        let display = chord.display(platform);
        ui.label(tr_args(
            "controls-binding-chip",
            &[("binding", display.clone())],
        ));

        if let Some(other) = conflict_partner {
            let message = conflict_message(controller, &other);
            ui.label(format!("⚠ {message}"));
        }

        let remove = ui.small_button("×");
        let remove_name = tr_args("controls-remove-binding", &[("binding", display)]);
        remove.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, remove_name.clone()));
        if remove.clicked() {
            controller.remove_binding(action.clone(), chord);
        }
    });
}

/// Per-`egui::Memory` id: does the capture control shown this frame need
/// to move keyboard focus onto itself (the frame it opens, or the frame a
/// different row's "Add binding" retargets it) — set once per activation,
/// cleared once the control has actually taken focus (mirrors `settings/
/// account.rs`'s `modal_focus_pending_id`).
fn capture_focus_pending_id() -> Id {
    Id::new("controls-capture-focus-pending")
}

fn open_capture(ui: &Ui, screen: &mut ControlsScreen, action: ActionId) {
    screen.capture = Some(action);
    screen.capture_error = None;
    ui.memory_mut(|memory| memory.data.insert_temp(capture_focus_pending_id(), true));
}

fn cancel_capture(ui: &Ui, screen: &mut ControlsScreen) {
    screen.capture = None;
    screen.capture_error = None;
    ui.memory_mut(|memory| memory.data.remove::<bool>(capture_focus_pending_id()));
}

/// The open capture control (data-model.md §4.3 transition table;
/// contracts/ui-actions.md §5): a focus-locked button showing
/// `controls-capture-prompt`, accessible name `controls-capture { $action }`.
/// `Esc` or losing focus (on any frame after the one it first requested
/// it) cancels with no change; the first `Event::Key` this frame is
/// checked against [`CaptureRule`] and either binds it (`add_binding`,
/// capture closes) or shows the inline rejection with capture left open.
fn show_capture_control<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    screen: &mut ControlsScreen,
    action: ActionId,
    existing: &[Chord],
) {
    let accessible = tr_args(
        "controls-capture",
        &[("action", action_label(controller, &action))],
    );
    let response = ui.add(Button::new(tr("controls-capture-prompt")));
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, accessible.clone()));
    actions::register_claim(ui.ctx(), response.id, Claim::TextLike);
    ui.memory_mut(|memory| {
        memory.set_focus_lock_filter(
            response.id,
            EventFilter {
                tab: true,
                horizontal_arrows: true,
                vertical_arrows: true,
                escape: true,
            },
        );
    });

    let focus_pending = ui
        .memory(|memory| memory.data.get_temp::<bool>(capture_focus_pending_id()))
        .unwrap_or(false);
    if focus_pending {
        response.request_focus();
    }

    if !focus_pending && !response.has_focus() {
        // Focus lost on a later frame (click elsewhere, window blur):
        // cancel with no change (contracts/ui-actions.md §5 step 1).
        cancel_capture(ui, screen);
        return;
    }
    ui.memory_mut(|memory| memory.data.insert_temp(capture_focus_pending_id(), false));

    let is_mac = ui.ctx().os().is_mac();
    let key_event = ui.input(|input| {
        input
            .events
            .iter()
            .find(|event| matches!(event, Event::Key { pressed: true, .. }))
            .cloned()
    });
    let Some(event) = key_event else {
        show_capture_error(ui, screen);
        return;
    };

    if matches!(
        &event,
        Event::Key {
            key: Key::Escape,
            ..
        }
    ) {
        cancel_capture(ui, screen);
        return;
    }

    match CaptureRule::check(&event, is_mac, existing) {
        Ok(chord) => {
            let _ = controller.add_binding(action.clone(), chord);
            cancel_capture(ui, screen);
            // US3 (FR-007's "accepted and immediately produces the
            // conflict"): the chip loop above already ran against the
            // pre-add snapshot, so the just-added chord's own chip won't
            // carry its `⚠` until next frame — surface the conflict-
            // partner name inline, right here, in this same frame.
            if let Some(other) = controller.actions().conflict_partner(action.clone(), chord) {
                let display = chord.display(if ui.ctx().os().is_mac() {
                    Platform::Mac
                } else {
                    Platform::Other
                });
                let message = conflict_message(controller, &other);
                ui.label(format!(
                    "{} ⚠ {message}",
                    tr_args("controls-binding-chip", &[("binding", display)])
                ));
            }
        }
        Err(reject) => {
            screen.capture_error = Some(reject.message_key());
        }
    }
    show_capture_error(ui, screen);
}

fn show_capture_error(ui: &mut Ui, screen: &ControlsScreen) {
    if let Some(key) = screen.capture_error {
        ui.label(tr(key));
    }
}

/// Per-`egui::Memory` id: the page-level "Reset all to defaults" confirm
/// pair's own focus-pending flag (same lifecycle as
/// [`capture_focus_pending_id`]).
fn reset_all_focus_pending_id() -> Id {
    Id::new("controls-reset-all-focus-pending")
}

/// "Reset all to defaults" (FR-011): a plain button until activated, then
/// a two-step inline confirm (`controls-reset-all-confirm` + Confirm/
/// Cancel); `Esc` or focus leaving both buttons cancels back to the plain
/// button with no change (contracts/ui-actions.md §4, data-model.md §4.3).
fn show_reset_all<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    screen: &mut ControlsScreen,
) {
    if !screen.reset_all_confirm {
        if ui.button(tr("controls-reset-all")).clicked() {
            screen.reset_all_confirm = true;
            ui.memory_mut(|memory| memory.data.insert_temp(reset_all_focus_pending_id(), true));
        }
        return;
    }

    let focus_pending = ui
        .memory(|memory| memory.data.get_temp::<bool>(reset_all_focus_pending_id()))
        .unwrap_or(false);

    ui.horizontal(|ui| {
        ui.label(tr("controls-reset-all-confirm"));
        let confirm = ui.button(tr("controls-confirm"));
        if focus_pending {
            confirm.request_focus();
        }
        let cancel = ui.button(tr("controls-cancel"));
        let escape_pressed = ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape));

        if confirm.clicked() {
            controller.reset_all_bindings();
            screen.reset_all_confirm = false;
        } else if cancel.clicked()
            || escape_pressed
            || (!focus_pending && !confirm.has_focus() && !cancel.has_focus())
        {
            screen.reset_all_confirm = false;
        }
    });

    if screen.reset_all_confirm {
        ui.memory_mut(|memory| memory.data.insert_temp(reset_all_focus_pending_id(), false));
    } else {
        ui.memory_mut(|memory| memory.data.remove::<bool>(reset_all_focus_pending_id()));
    }
}

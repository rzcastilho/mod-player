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
    Button, Event, EventFilter, Id, Key, Modifiers, Rect, Response, Sense, TextEdit, Ui,
    WidgetInfo, WidgetType,
};
use modplayer_audio_io::OutputBackend;
use modplayer_audio_source::SourceHost;
use modplayer_core::actions::{ActionId, ActionKind, Chord, Platform};
use modplayer_core::{PlaybackController, tr, tr_args};

use crate::actions::{self, Claim, key_name_from_egui, mods_from_egui};
use crate::key_label::KeyLabel;
use crate::shortcut_groups;
use crate::theme;

/// Draw one `section`-role heading (014-design-tokens-and-type-scale, US2,
/// T033, data-model.md §6 "settings groups" -> `theme::section_label`), and
/// pin its accesskit accessible name back to the exact, un-uppercased
/// `text` — mirroring `markers.rs`'s `panel()` (T030) — so
/// `rendered_texts`-style exact-text assertions (`controls.rs` tests) and
/// FR-019 ("no behaviour change") both still see the original string, even
/// though the painted text is now uppercase+tracked.
fn section_heading(ui: &mut Ui, text: &str) {
    let response = ui.label(theme::section_label(text));
    ui.ctx().accesskit_node_builder(response.id, |b| {
        b.set_label(text);
    });
}

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

/// `id`'s display label, resolved the same way the shortcut snapshot resolves
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
    let platform = shortcut_groups::platform_of(ui.ctx());
    let groups = shortcut_groups::snapshot(controller.actions());

    section_heading(ui, &tr("controls-heading"));

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
    theme::divider(ui);

    let visible = shortcut_groups::filter_groups(&groups, &screen.filter, platform);

    if visible.is_empty() {
        ui.label(tr("controls-no-match"));
        return;
    }

    // The page's floating scroll bar overlays the right edge; keep the
    // groups (and the right column's wrapped conflict notes) clear of it.
    // A child UI, because the full-width divider above already widened
    // this one's min rect, so `set_max_width` could not narrow it.
    let scroll = ui.spacing().scroll;
    let mut rect = ui.available_rect_before_wrap();
    rect.max.x = (rect.max.x - scroll.bar_width - scroll.bar_outer_margin).max(rect.min.x);
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
        if shortcut_groups::use_two_columns(ui.available_width()) {
            let weights: Vec<usize> = visible.iter().map(|(_, rows)| 1 + rows.len()).collect();
            let split = shortcut_groups::column_break(&weights);
            let (left, right) = visible.split_at(split);
            ui.columns(2, |columns| {
                for (column, part) in columns.iter_mut().zip([left, right]) {
                    for (group, rows) in part {
                        show_group(column, controller, screen, group, rows, platform);
                    }
                }
            });
        } else {
            for (group, rows) in &visible {
                show_group(ui, controller, screen, group, rows, platform);
            }
        }
    });
}

fn show_group<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    screen: &mut ControlsScreen,
    group: &shortcut_groups::ShortcutGroup,
    rows: &[shortcut_groups::FilteredRow],
    platform: Platform,
) {
    section_heading(ui, &group.heading);
    for filtered in rows {
        show_row(ui, controller, screen, filtered, platform);
    }
}

fn filter_box_id() -> Id {
    Id::new("controls-filter-box")
}

/// A row's reveal bookkeeping, kept in `egui::Memory` between frames
/// (research R4): the row's rect last frame, whether focus sat on the
/// row element or any of its controls, and whether the controls were
/// shown.
#[derive(Debug, Clone, Copy)]
struct RowState {
    rect: Rect,
    focus_within: bool,
    revealed: bool,
}

/// One action's row: label (+ "(inactive)" when disabled/the owning
/// plugin is not Active, de-emphasized — contracts/action-registry-
/// plugins.md G15 "a row whose plugin is not Active renders greyed"),
/// a kind label for continuous actions only, one chip per binding, and —
/// only while the row is revealed (pointer over it, focus within it, or
/// a capture open on it) — `×` per chip, "Add binding" (or the open
/// capture control) and "Reset to default" (contracts C6–C13).
fn show_row<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    screen: &mut ControlsScreen,
    filtered: &shortcut_groups::FilteredRow,
    platform: Platform,
) {
    let row = filtered.row;
    let conflict_notes: Vec<(Chord, String)> = row
        .conflicts
        .iter()
        .filter_map(|&chord| {
            controller
                .actions()
                .conflict_partner(row.id.clone(), chord)
                .map(|other| (chord, conflict_message(controller, &other)))
        })
        .collect();

    let state_id = Id::new(("controls-row-state", &row.id));
    let previous: Option<RowState> = ui.memory(|memory| memory.data.get_temp(state_id));
    let pointer = ui.input(|input| input.pointer.hover_pos());
    let hovered = previous
        .zip(pointer)
        .is_some_and(|(state, pos)| state.rect.contains(pos));
    let capturing = screen.capture.as_ref() == Some(&row.id);
    let revealed = capturing || hovered || previous.is_some_and(|state| state.focus_within);

    let spoken_bindings = if row.bindings.is_empty() {
        tr("shortcuts-not-bound")
    } else {
        row.bindings
            .iter()
            .map(|&chord| KeyLabel::new(chord, platform).spoken)
            .collect::<Vec<_>>()
            .join(", ")
    };

    let mut focus_within = false;
    let out = ui.horizontal_wrapped(|ui| {
        let label = row.label.clone();
        let name =
            shortcut_groups::paint_name_with_sense(ui, row, Sense::focusable_noninteractive());
        let row_name = tr_args(
            "controls-row",
            &[
                ("action", label.clone()),
                ("bindings", spoken_bindings.clone()),
            ],
        );
        ui.ctx().accesskit_node_builder(name.id, |b| {
            b.set_label(row_name.clone());
        });
        focus_within |= name.has_focus();

        if row.kind == ActionKind::Continuous {
            ui.label(tr("controls-kind-continuous"));
        }

        shortcut_groups::paint_bindings(
            ui,
            row,
            platform,
            &conflict_notes,
            &filtered.key_hits,
            &mut |ui, chord| {
                if revealed {
                    let remove = show_remove(ui, controller, row.id.clone(), chord, platform);
                    focus_within |= remove.has_focus();
                }
            },
        );

        if capturing {
            show_capture_control(ui, controller, screen, row.id.clone(), &row.bindings);
        } else if revealed {
            let add = ui.button(tr("controls-add-binding"));
            focus_within |= add.has_focus();
            if add.clicked() {
                open_capture(ui, screen, row.id.clone());
            }
        }
        if revealed {
            let reset_label = tr_args("controls-reset-action", &[("action", label)]);
            let reset = ui.button(reset_label);
            focus_within |= reset.has_focus();
            if reset.clicked() {
                controller.reset_binding(row.id.clone());
            }
        }
    });

    let state = RowState {
        rect: out.response.rect,
        focus_within,
        revealed,
    };
    if previous.is_some_and(|p| p.revealed != revealed) {
        ui.ctx().request_repaint();
    }
    ui.memory_mut(|memory| memory.data.insert_temp(state_id, state));
}

/// The `×` button of one binding chip: removes it immediately (FR-008).
/// The chip label and its `⚠` conflict note are painted by
/// [`shortcut_groups::paint_bindings`].
fn show_remove<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    action: ActionId,
    chord: Chord,
    platform: Platform,
) -> Response {
    let remove = ui.small_button("×");
    let remove_name = tr_args(
        "controls-remove-binding",
        &[("binding", KeyLabel::new(chord, platform).spoken)],
    );
    remove.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, remove_name.clone()));
    if remove.clicked() {
        controller.remove_binding(action, chord);
    }
    remove
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
                let display = KeyLabel::new(chord, shortcut_groups::platform_of(ui.ctx())).visible;
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

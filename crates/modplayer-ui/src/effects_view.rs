// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Effect Chain panel (008, contracts/ui-effect-chain.md): opened via
//! `E`/the header toggle beside "Queue" in Now Playing, lists nodes in
//! processing order with type/owner, a bypass toggle, a drag handle
//! (pointer drag-and-drop and `↑`/`↓`), a per-node CPU figure, and
//! per-kind parameter controls, plus an "Add node…" control that refuses
//! inline at capacity.
//!
//! Phase 5 (T073) completes the full six-kind catalog's controls:
//! equalizer (8 bands), filter and stereo tools. The live meters/
//! spectrum/overload header figures land in Phase 6 (T089). The
//! "Add node…" selection is kept in egui temp memory (a per-viewer
//! convenience) rather than an app-owned state struct, so no other screen
//! needs to thread a new field through to reach this one.

use egui::{ComboBox, CursorIcon, DragValue, Id, Key, Label, Modifiers, Slider, Ui};
use modplayer_audio_io::OutputBackend;
use modplayer_audio_source::SourceHost;
use modplayer_core::{
    ChainView, MeterSnapshot, NodeId, NodeRow, NowPlayingPanel, PlaybackController, tr, tr_args,
};
use modplayer_effects::catalog::{NodeKind, NodeOwner, ParamId, QualityMode};

use crate::actions::{self, Claim};
use crate::theme;
use crate::theme::controls::Variant;
use crate::widgets::chain_meters;
use crate::widgets::controls::{
    CardResponse, SwitchKind, button, collapsible_panel_card, destructive_gap, switch,
};

/// The kinds the "Add node…" control offers (contracts/ui-effect-
/// chain.md §2: "a `ComboBox` of the six kinds").
const ADDABLE_KINDS: [NodeKind; 6] = NodeKind::ALL;

/// `E` (007's `HostAction::ToggleEffectChain`, contracts/ui-effect-chain.md
/// §1): flips the persisted `[now_playing_panels] effect_chain_open` flag
/// through the controller, so a keyboard toggle and the header switch stay
/// in sync and the state survives a restart
/// (016-list-row-and-panel-components, FR-019). Returns the panel's new
/// open state (021-transport-bar-and-panel-layout, contract R5, research
/// R6): `true` tells the caller to also request a reveal.
pub fn toggle_effect_chain_panel<B: OutputBackend, H: SourceHost>(
    controller: &mut PlaybackController<B, H>,
) -> bool {
    let open = controller.now_playing_panel_open(NowPlayingPanel::EffectChain);
    let new_open = !open;
    controller.set_now_playing_panel_open(NowPlayingPanel::EffectChain, new_open);
    new_open
}

/// The egui memory id the "Add node…" combo's own selection persists
/// under (a per-viewer convenience, unrelated to the panel's own
/// persisted open/closed state).
fn add_kind_memory_id() -> Id {
    Id::new("now-playing-effect-chain-add-kind")
}

/// A node's drag handle's id — stable across frames, keyed by [`NodeId`]
/// rather than row index, so both pointer-drag registration and keyboard
/// focus survive a reorder (contracts/ui-effect-chain.md §4). `pub` so
/// `handle_focused_handle_keys`'s callers (and this module's own tests)
/// can address a specific node's handle directly.
#[must_use]
pub fn handle_id(node: NodeId) -> Id {
    Id::new("effects-handle").with(node.as_u32())
}

/// Draw the Effect Chain panel: header, one row per node in processing
/// order, then the "Add node…" control. Rendered regardless of whether a
/// track is loaded (contracts/ui-effect-chain.md §1 — an empty chain with
/// 0 % figures is valid; 021-transport-bar-and-panel-layout contract C4:
/// always present, either open or as a header-only collapsed card).
pub fn show<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    open: &mut bool,
) -> CardResponse {
    let view = controller.chain_view();
    let meters = controller.chain_meters();

    // 016-list-row-and-panel-components (FR-017/FR-018, research R7),
    // superseded by 021 contract C1/C2 (research R7): the shared
    // collapsible card now draws the `effects-panel-title` header plus a
    // header disclosure — the in-row `section_label` this panel used to
    // draw itself stays deleted, so it is never rendered twice. The CPU/
    // overload/over-budget controls keep their own row inside the card.
    collapsible_panel_card(ui, &tr("effects-panel-title"), open, |ui| {
        show_header(ui, &view, &meters);

        // 024-effect-chain-rows-and-meters (US3, contract R4.1): header,
        // level pairs and spectrum render first either way; an empty chain
        // then swaps the row list + ordinary add row for the collapsed/
        // revealed empty state instead (the swap happens in the frame
        // `view.nodes.len()` changes, since `view` is rebuilt fresh above).
        if view.nodes.is_empty() {
            show_empty_state(ui, controller);
        } else {
            for row in &view.nodes {
                show_row(ui, controller, row);
            }

            // R4.5: once the chain has at least one node, the empty
            // state's own reveal/focus flags are cleared every frame, so
            // the next time it empties it starts collapsed again.
            clear_empty_add_state(ui);
            show_add_row(ui, controller, view.nodes.len(), view.capacity);
        }
    })
}

/// The panel header's chrome-under-the-title row (contracts/ui-effect-
/// chain.md §2): whole-chain CPU figure, overload counter, an over-budget
/// badge while `over_budget`, the pre-/post-chain level pairs and the
/// 64-band spectrum — all read live from `RtShared` via `ChainView`/
/// `MeterSnapshot` every frame. The title itself is drawn by the
/// surrounding `panel_card`.
fn show_header(ui: &mut Ui, view: &ChainView, meters: &MeterSnapshot) {
    ui.horizontal(|ui| {
        // 024-effect-chain-rows-and-meters (contract H1/H2, FR-006): the
        // whole-chain figure reads as budget-relative ("… of real-time
        // budget"), mono so the right-padded `{:>3.0}` digits line up, and
        // carries a hover explanation of what "budget" means.
        let cpu = ui.label(theme::mono_text(tr_args(
            "effects-chain-cpu",
            &[("pct", format!("{:>3.0}", view.total_cost_pct))],
        )));
        cpu.on_hover_text(tr("effects-chain-cpu-hint"));

        ui.label(theme::mono_text(tr_args(
            "effects-overloads",
            &[("count", view.overload_count.to_string())],
        )));
        if view.over_budget {
            ui.label(tr("effects-over-budget-badge"));
        }
    });
    // One pair per line: each `level_pair` is an atomic `horizontal` a
    // wrapping row can't break, and side by side they overflowed the
    // centre column, widening every row below them (2026-09-28 manual
    // walk, research R15).
    chain_meters::level_pair(ui, "effects-pre", meters.pre);
    chain_meters::level_pair(ui, "effects-post", meters.post);
    chain_meters::spectrum(ui, &meters.spectrum);
}

/// Zone indices into the [`zone_breaks`] array (data-model.md §2,
/// contracts/ui-effect-chain-rows.md R1): Identity, State, Parameters,
/// Actions, in tab order.
const ZONE_IDENTITY: usize = 0;
const ZONE_STATE: usize = 1;
const ZONE_PARAMETERS: usize = 2;
const ZONE_ACTIONS: usize = 3;
const ZONE_COUNT: usize = 4;

/// The identity zone's kind+owner text budget (contract R1.6, research
/// R10) — the handle glyph and position number sit outside this budget.
const IDENTITY_TEXT_MAX_WIDTH: f32 = 180.0;

/// The reserved inter-zone space two zones sharing one line put between
/// them (research R2): a vertical separator rule plus `theme::space::LG`.
/// Approximate — the separator's own stroke width is a couple of px, which
/// [`zone_breaks`]'s fit test does not need to the pixel to keep zones from
/// visibly crowding.
const ROW_ZONE_GAP: f32 = theme::space::LG;

/// Decide, for each of a row's four zones, whether it must start a new
/// line (data-model.md §4.1, contracts/ui-effect-chain-rows.md R1.2/R1.3).
/// `widths[i]` is last frame's measured width for zone `i`, or `None` the
/// first time that zone is ever drawn for this node. Zone 0 (Identity)
/// never breaks — it always opens the row's first line (a zone is never
/// split across lines: the break rule only ever runs *before* a zone).
fn zone_breaks(available: f32, widths: [Option<f32>; ZONE_COUNT], gap: f32) -> [bool; ZONE_COUNT] {
    let mut breaks = [false; ZONE_COUNT];
    let mut line_width = widths[ZONE_IDENTITY].unwrap_or(0.0);
    for zone in ZONE_STATE..ZONE_COUNT {
        let Some(width) = widths[zone] else {
            // Unmeasured (first frame): Parameters always starts its own
            // line (R1.3, today's `end_row()` behaviour); every other
            // zone's fit test assumes zero width, so it never forces a
            // break on a first frame.
            if zone == ZONE_PARAMETERS {
                breaks[zone] = true;
                line_width = 0.0;
            }
            continue;
        };
        if line_width + gap + width > available {
            breaks[zone] = true;
            line_width = width;
        } else {
            line_width += gap + width;
        }
    }
    breaks
}

/// The egui temp-memory id a zone's last-measured width is kept under
/// (data-model.md §3): `f32`, per `(node, zone)`, read by [`zone_breaks`]
/// and rewritten every frame after that zone is drawn.
fn zone_width_id(node: NodeId, zone: usize) -> Id {
    Id::new("effects-zone-width").with((node.as_u32(), zone))
}

fn read_zone_width(ui: &Ui, node: NodeId, zone: usize) -> Option<f32> {
    ui.memory(|memory| memory.data.get_temp::<f32>(zone_width_id(node, zone)))
}

fn write_zone_width(ui: &Ui, node: NodeId, zone: usize, width: f32) {
    ui.memory_mut(|memory| memory.data.insert_temp(zone_width_id(node, zone), width));
}

/// One node's row (contracts/ui-effect-chain-rows.md R1): a `dnd_drop_zone`
/// wrapping the row's four zones — Identity, State, Parameters, Actions,
/// in that order — each an atomic `ui.horizontal` [`zone_breaks`] never
/// splits across lines. Adjacent zones sharing one line are separated by a
/// vertical separator rule plus `theme::space::LG` (R1.1); Actions is
/// right-aligned when it shares its line with Parameters (R1.4). A pointer
/// drop of another row's handle onto this one reorders via
/// `chain_move_node`.
fn show_row<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    row: &NodeRow,
) {
    let (_frame, payload) = ui.dnd_drop_zone::<NodeId, _>(egui::Frame::group(ui.style()), |ui| {
        // Wrapped, not a plain `horizontal`: inside 021's single vertical
        // scroll region a row wider than the centre column painted over
        // the plugin dock (2026-09-28 manual walk, research R15).
        let available = ui.available_width();
        let widths = [
            read_zone_width(ui, row.id, ZONE_IDENTITY),
            read_zone_width(ui, row.id, ZONE_STATE),
            read_zone_width(ui, row.id, ZONE_PARAMETERS),
            read_zone_width(ui, row.id, ZONE_ACTIONS),
        ];
        let breaks = zone_breaks(available, widths, ROW_ZONE_GAP);
        // R1.4: Actions is right-aligned only when it shares its line with
        // Parameters — true exactly when Actions itself doesn't break,
        // since nothing but Parameters can ever precede it on that line.
        let actions_right_aligned = !breaks[ZONE_ACTIONS];

        ui.vertical(|ui| {
            let mut zone = ZONE_IDENTITY;
            while zone < ZONE_COUNT {
                ui.horizontal(|ui| {
                    loop {
                        let right_align = zone == ZONE_ACTIONS && actions_right_aligned;
                        let width = draw_zone(ui, controller, row, zone, right_align);
                        write_zone_width(ui, row.id, zone, width);
                        zone += 1;
                        if zone >= ZONE_COUNT || breaks[zone] {
                            break;
                        }
                        // R1.1/R1.7: the separator (and its spacing) is drawn
                        // between every pair of same-line zones regardless of
                        // whether the zone just drawn had any content, so
                        // Parameters keeps its separator slot even for a kind
                        // with no controls (Edge Cases).
                        ui.separator();
                        ui.add_space(theme::space::LG);
                    }
                });
            }
        });
    });

    if let Some(dragged) = payload
        && *dragged != row.id
    {
        let _ = controller.chain_move_node(*dragged, row.index);
    }
}

/// Draws zone `zone` of `row` and returns its measured width (fed back
/// into next frame's [`zone_breaks`] via [`write_zone_width`]).
fn draw_zone<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    row: &NodeRow,
    zone: usize,
    right_align_actions: bool,
) -> f32 {
    let response = match zone {
        ZONE_IDENTITY => show_identity_zone(ui, row),
        ZONE_STATE => show_state_zone(ui, controller, row),
        ZONE_PARAMETERS => show_parameters_zone(ui, controller, row),
        ZONE_ACTIONS => show_actions_zone(ui, controller, row, right_align_actions),
        _ => unreachable!("show_row only ever asks for one of the four zones"),
    };
    response.rect.width()
}

/// Zone 1, Identity (contract R1 row 1): the drag handle, then the
/// truncating `"{index+1}. {kind}"` / owner labels, capped at
/// [`IDENTITY_TEXT_MAX_WIDTH`] combined (R1.6, research R10) with the full
/// text on hover when elided (`Label`'s own built-in tooltip-when-elided).
fn show_identity_zone(ui: &mut Ui, row: &NodeRow) -> egui::Response {
    ui.horizontal(|ui| {
        show_handle(ui, row);
        ui.scope(|ui| {
            ui.set_max_width(IDENTITY_TEXT_MAX_WIDTH);
            ui.add(
                Label::new(format!("{}. {}", row.index + 1, tr(row.kind.label_key()))).truncate(),
            );
            ui.add(Label::new(tr(owner_label_key(row.owner))).truncate());
        });
    })
    .response
}

/// Zone 2, State (contract R1 row 2): the Bypass toggle, this node's CPU
/// figure, and — moved here from the Parameters zone (R1.5) — the
/// auto-bypassed/quality-mode-auto-switched notes.
fn show_state_zone<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    row: &NodeRow,
) -> egui::Response {
    ui.horizontal(|ui| {
        let mut bypassed = row.bypassed;
        if switch(ui, SwitchKind::Toggle, &mut bypassed, &tr("effects-bypass")).changed() {
            let _ = controller.chain_set_bypass(row.id, bypassed);
        }

        // 024-effect-chain-rows-and-meters (contract H5, FR-006): same
        // budget-relative wording and fixed-width `{:>3.0}` padding as the
        // header's whole-chain figure, mono.
        ui.label(theme::mono_text(tr_args(
            "effects-cpu",
            &[("pct", format!("{:>3.0}", row.cost_pct))],
        )));

        if row.auto_bypassed {
            // 014-design-tokens-and-type-scale (US2, T028): a
            // supplementary status note, `secondary`/`.weak()` like a
            // row's other detail text.
            ui.label(egui::RichText::new(tr("effects-auto-bypassed")).weak());
        }
        if row.mode_note {
            ui.label(egui::RichText::new(tr("effects-mode-note")).weak());
        }
    })
    .response
}

/// Zone 3, Parameters (contract R1 row 3): this kind's controls, exactly
/// as 008 (ranges, steps, suffixes, combos, `add_enabled` for phase
/// invert) — unchanged behavior (FR-014), just its own atomic zone now.
/// Wrapped, not a plain `horizontal`: a kind whose controls are wider
/// than the row (Stereo tools in the ~740 pt centre column of a 960 pt
/// window) wraps its own controls instead of pushing the row, the card
/// and the scroll region past the window edge (contract R5, T043 M8). A
/// zone that wraps measures the full row width, so [`zone_breaks`] then
/// keeps it on a line of its own.
fn show_parameters_zone<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    row: &NodeRow,
) -> egui::Response {
    ui.horizontal_wrapped(|ui| {
        show_params(ui, controller, row);
    })
    .response
}

/// Zone 4, Actions (contract R1 row 4, R1.1/R1.4): `destructive_gap`
/// always immediately precedes Remove (one call site, matching every
/// other named `destructive_gap` instance in the design system). When
/// `right_align` (Actions shares its line with Parameters), that pair is
/// pushed to the line's trailing edge by a leading space sized from last
/// frame's own remembered content width — the same "measure, then use
/// next frame" technique [`zone_breaks`] itself relies on — rather than
/// reversing the add order inside a `Layout::right_to_left`, which would
/// put the gap on the wrong side of Remove (between it and the outer
/// edge, not between it and Parameters).
fn show_actions_zone<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    row: &NodeRow,
    right_align: bool,
) -> egui::Response {
    let mut draw_content = |ui: &mut Ui| -> egui::Response {
        ui.horizontal(|ui| {
            destructive_gap(ui);
            if button(ui, Variant::Destructive, tr("effects-remove")).clicked() {
                let _ = controller.chain_remove_node(row.id);
            }
        })
        .response
    };

    if right_align {
        ui.horizontal(|ui| {
            if let Some(width) = read_zone_width(ui, row.id, ZONE_ACTIONS) {
                ui.add_space((ui.available_width() - width).max(0.0));
            }
            draw_content(ui)
        })
        .inner
    } else {
        draw_content(ui)
    }
}

/// The drag handle (contracts/ui-effect-chain-rows.md R2): a focusable
/// six-dot grip ([`paint_grip`], research R3/R15), visible without hover, that starts a pointer drag
/// carrying this node's [`NodeId`] as the DnD payload, and claims `↑`/`↓`
/// (`effect_handle_claims`, `handle_focused_handle_keys` applies them) so
/// the dispatcher leaves them for this widget while it has focus. Pointer
/// hover already gets `CursorIcon::Grab` from `dnd_drag_source` itself
/// (egui, research R3); this additionally sets `CursorIcon::Grabbing`
/// while its own drag is active (R2.2), a hover tooltip (R2.5), and an
/// AccessKit name that names both the node's kind and its 1-based
/// position (R2.4) so N handles stay distinguishable (NFR-6.2).
fn show_handle(ui: &mut Ui, row: &NodeRow) {
    let id = handle_id(row.id);
    let roles = theme::roles(ui.visuals());
    let response = ui
        .dnd_drag_source(id, row.id, |ui| {
            paint_grip(ui, roles.text_secondary);
        })
        .response;
    if ui.ctx().is_being_dragged(id) {
        ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
    }
    let response = response.on_hover_text(tr("effects-reorder-handle-hint"));
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            true,
            tr_args(
                "effects-reorder-handle-node",
                &[
                    ("kind", tr(row.kind.label_key())),
                    ("position", (row.index + 1).to_string()),
                ],
            ),
        )
    });
    actions::register_claim(ui.ctx(), id, Claim::Keys(actions::effect_handle_claims()));
    // Stop egui's own spatial arrow-key focus navigation from stealing
    // focus to a geometrically-"up"/"down" widget on `↑`/`↓` (mirrors
    // `markers.rs`'s glyph, which locks `horizontal_arrows` for the same
    // reason) — this handle's own `↑`/`↓` mean "move this node", not
    // "move focus".
    ui.memory_mut(|memory| {
        memory.set_focus_lock_filter(
            id,
            egui::EventFilter {
                vertical_arrows: true,
                ..egui::EventFilter::default()
            },
        );
    });
}

/// The grip's dot radius — [`paint_grip`] spaces its dots
/// `theme::space::XS` apart, so this leaves a clear gap between them.
const GRIP_DOT_RADIUS: f32 = 1.25;

/// The drag handle's grip (research R3/R15): a six-dot 2×3 grid painted
/// in `color`, in place of the braille "⠿" (U+283F) glyph — egui's
/// bundled fonts don't carry it, so it rendered as a tofu box (T043 D1).
/// One body line tall, so the identity zone's height is unchanged.
fn paint_grip(ui: &mut Ui, color: egui::Color32) {
    let height = ui.text_style_height(&egui::TextStyle::Body);
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(theme::space::MD, height), egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        let pitch = theme::space::XS;
        for column in [-0.5_f32, 0.5] {
            for line in [-1.0_f32, 0.0, 1.0] {
                let center = rect.center() + egui::vec2(column * pitch, line * pitch);
                ui.painter().circle_filled(center, GRIP_DOT_RADIUS, color);
            }
        }
    }
}

/// `↑`/`↓` for whichever node's handle currently has keyboard focus
/// (contracts/ui-effect-chain.md §4): moves it one position and keeps
/// focus on the same node's handle (its id is keyed by `NodeId`, not row
/// index, so redrawing it at the new position preserves focus
/// automatically). Call once per frame, while the panel is open, after
/// [`show`].
pub fn handle_focused_handle_keys<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
) {
    let Some(focused) = ui.memory(|memory| memory.focused()) else {
        return;
    };
    let Some(id) = controller
        .chain()
        .nodes()
        .iter()
        .find(|node| handle_id(node.id) == focused)
        .map(|node| node.id)
    else {
        return;
    };
    if ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::ArrowUp)) {
        let _ = controller.chain_move_node_by(id, -1);
    } else if ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::ArrowDown)) {
        let _ = controller.chain_move_node_by(id, 1);
    }
}

/// `effects-owner-host` / `effects-owner-plugin` (data-model.md §1.3 —
/// only `Host` is reachable from this controller in this slice;
/// `Plugin(_)` is exercised by engine tests ahead of 009's runtime).
const fn owner_label_key(owner: NodeOwner) -> &'static str {
    if owner.is_host() {
        "effects-owner-host"
    } else {
        "effects-owner-plugin"
    }
}

/// This node's parameter controls (contracts/ui-effect-chain.md §3).
/// Every change goes through `chain_set_param`/`chain_set_mode` — the
/// next frame's `ChainView` (built fresh from the controller at the top
/// of [`show`]) already reflects the clamped value (SC-005), so no
/// same-frame snap-back is needed here.
fn show_params<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    row: &NodeRow,
) {
    match row.kind {
        NodeKind::PitchShift => show_pitch_shift_params(ui, controller, row),
        NodeKind::TimeStretch => show_time_stretch_params(ui, controller, row),
        NodeKind::Gain => show_gain_params(ui, controller, row),
        NodeKind::Equalizer => show_equalizer_params(ui, controller, row),
        NodeKind::Filter => show_filter_params(ui, controller, row),
        NodeKind::StereoTools => show_stereo_params(ui, controller, row),
    }
}

/// Pitch shift: semitones slider, formant toggle, mode combo (params
/// indices 0/1/2, catalog.rs's `PITCH_SHIFT_PARAMS`).
fn show_pitch_shift_params<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    row: &NodeRow,
) {
    let mut semitones = row.params[0];
    if ui
        .add(
            Slider::new(&mut semitones, -12.0..=12.0)
                .step_by(0.01)
                .suffix(" st")
                .text(tr("effects-param-semitones")),
        )
        .changed()
    {
        let _ = controller.chain_set_param(row.id, ParamId(0), semitones);
    }

    let mut formant = row.params[1] != 0.0;
    if switch(
        ui,
        SwitchKind::Toggle,
        &mut formant,
        &tr("effects-param-formant"),
    )
    .changed()
    {
        let _ = controller.chain_set_param(row.id, ParamId(1), f32::from(formant));
    }

    show_mode_combo(ui, controller, row, row.params[2]);
}

/// Time stretch: ratio slider shown as a 25..200 % percentage (writes
/// `ratio = pct / 100`), mode combo (params indices 0/1,
/// `TIME_STRETCH_PARAMS`).
fn show_time_stretch_params<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    row: &NodeRow,
) {
    let mut pct = row.params[0] * 100.0;
    if ui
        .add(
            Slider::new(&mut pct, 25.0..=200.0)
                .suffix(" %")
                .text(tr("effects-param-ratio")),
        )
        .changed()
    {
        let _ = controller.chain_set_param(row.id, ParamId(0), pct / 100.0);
    }

    show_mode_combo(ui, controller, row, row.params[1]);
}

/// Gain: level slider (dB), mute toggle (params indices 0/1,
/// `GAIN_PARAMS`).
fn show_gain_params<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    row: &NodeRow,
) {
    let mut level = row.params[0];
    if ui
        .add(
            Slider::new(&mut level, -60.0..=12.0)
                .suffix(" dB")
                .text(tr("effects-param-level")),
        )
        .changed()
    {
        let _ = controller.chain_set_param(row.id, ParamId(0), level);
    }

    let mut mute = row.params[1] != 0.0;
    if switch(ui, SwitchKind::Toggle, &mut mute, &tr("effects-param-mute")).changed() {
        let _ = controller.chain_set_param(row.id, ParamId(1), f32::from(mute));
    }
}

/// Equalizer: 8 bands, each a `DragValue` Hz (logarithmic speed) / dB /
/// Q plus a `ComboBox` type (params indices `4*band + {0,1,2,3}`,
/// `catalog.rs`'s `16 + 4*band + n` scheme collapsed to a plain
/// positional index within this kind's own params — data-model.md
/// §1.3). The bands stack vertically inside the row: laid end to end on
/// the row's own horizontal they ran off the right edge of a 1 560 px
/// window from band 4 on, leaving bands 5–8 unreachable (2026-09-19
/// manual walk, M4/M11).
fn show_equalizer_params<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    row: &NodeRow,
) {
    ui.vertical(|ui| {
        for band in 0u8..8 {
            show_equalizer_band(ui, controller, row, band);
        }
    });
}

/// One equalizer band's line: index, frequency, gain, Q and type.
fn show_equalizer_band<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    row: &NodeRow,
    band: u8,
) {
    let base = usize::from(band) * 4;
    ui.horizontal(|ui| {
        ui.label(format!("{}", band + 1));

        let mut freq = row.params[base];
        // Drag speed scales with the current value (a logarithmic
        // feel — a coarse-but-fast approximation, since `DragValue`
        // has no built-in log mode): ~1 Hz/px near 20 Hz, ~200 Hz/px
        // near 20 kHz.
        let freq_speed = f64::from(freq.max(20.0)) * 0.01;
        if ui
            .add(
                DragValue::new(&mut freq)
                    .range(20.0..=20_000.0)
                    .speed(freq_speed)
                    .suffix(" Hz")
                    .prefix(format!("{} ", tr("effects-param-band-freq"))),
            )
            .changed()
        {
            let _ = controller.chain_set_param(row.id, ParamId::eq_band(band, 0), freq);
        }

        let mut gain = row.params[base + 1];
        if ui
            .add(
                DragValue::new(&mut gain)
                    .range(-24.0..=24.0)
                    .suffix(" dB")
                    .prefix(format!("{} ", tr("effects-param-band-gain"))),
            )
            .changed()
        {
            let _ = controller.chain_set_param(row.id, ParamId::eq_band(band, 1), gain);
        }

        let mut q = row.params[base + 2];
        if ui
            .add(
                DragValue::new(&mut q)
                    .range(0.1..=10.0)
                    .speed(0.05)
                    .prefix(format!("{} ", tr("effects-param-band-q"))),
            )
            .changed()
        {
            let _ = controller.chain_set_param(row.id, ParamId::eq_band(band, 2), q);
        }

        show_band_type_combo(ui, controller, row, band, row.params[base + 3]);
    });
}

/// One EQ band's `type` combo (peak/low-shelf/high-shelf, data-model.md
/// §1.3).
fn show_band_type_combo<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    row: &NodeRow,
    band: u8,
    value: f32,
) {
    let current = value.round().clamp(0.0, 2.0) as u8;
    let mut selected = current;
    ComboBox::from_id_salt(("effects-band-type", row.id.as_u32(), band))
        .selected_text(tr(band_type_label_key(current)))
        .show_ui(ui, |ui| {
            for option in 0u8..3 {
                ui.selectable_value(&mut selected, option, tr(band_type_label_key(option)));
            }
        });
    if selected != current {
        let _ = controller.chain_set_param(row.id, ParamId::eq_band(band, 3), f32::from(selected));
    }
}

const fn band_type_label_key(band_type: u8) -> &'static str {
    match band_type {
        1 => "effects-band-type-low-shelf",
        2 => "effects-band-type-high-shelf",
        _ => "effects-band-type-peak",
    }
}

/// Filter: mode combo, cutoff `DragValue` (Hz), resonance `Slider`
/// (params indices 0/1/2, `FILTER_PARAMS`).
fn show_filter_params<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    row: &NodeRow,
) {
    show_filter_mode_combo(ui, controller, row, row.params[0]);

    let mut cutoff = row.params[1];
    let cutoff_speed = f64::from(cutoff.max(20.0)) * 0.01;
    if ui
        .add(
            DragValue::new(&mut cutoff)
                .range(20.0..=20_000.0)
                .speed(cutoff_speed)
                .suffix(" Hz")
                .prefix(format!("{} ", tr("effects-param-cutoff"))),
        )
        .changed()
    {
        let _ = controller.chain_set_param(row.id, ParamId(1), cutoff);
    }

    let mut resonance = row.params[2];
    if ui
        .add(Slider::new(&mut resonance, 0.0..=1.0).text(tr("effects-param-resonance")))
        .changed()
    {
        let _ = controller.chain_set_param(row.id, ParamId(2), resonance);
    }
}

fn show_filter_mode_combo<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    row: &NodeRow,
    value: f32,
) {
    let current = value >= 0.5; // false = high-pass, true = low-pass
    let mut selected = current;
    ComboBox::from_id_salt(("effects-filter-mode", row.id.as_u32()))
        .selected_text(tr(filter_mode_label_key(current)))
        .show_ui(ui, |ui| {
            for option in [false, true] {
                ui.selectable_value(&mut selected, option, tr(filter_mode_label_key(option)));
            }
        });
    if selected != current {
        let _ = controller.chain_set_param(row.id, ParamId(0), f32::from(selected));
    }
}

const fn filter_mode_label_key(low_pass: bool) -> &'static str {
    if low_pass {
        "effects-filter-low-pass"
    } else {
        "effects-filter-high-pass"
    }
}

/// Stereo tools: width/balance sliders, mono-sum/phase-invert/swap
/// toggles (params indices 0..5, `STEREO_TOOLS_PARAMS`). Phase invert is
/// only meaningful while mono sum is on (US3 AS6) — disabled otherwise
/// via `add_enabled`.
fn show_stereo_params<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    row: &NodeRow,
) {
    let mut width = row.params[0];
    if ui
        .add(Slider::new(&mut width, 0.0..=2.0).text(tr("effects-param-width")))
        .changed()
    {
        let _ = controller.chain_set_param(row.id, ParamId(0), width);
    }

    let mut balance = row.params[1];
    if ui
        .add(Slider::new(&mut balance, -1.0..=1.0).text(tr("effects-param-balance")))
        .changed()
    {
        let _ = controller.chain_set_param(row.id, ParamId(1), balance);
    }

    let mut mono_sum = row.params[2] != 0.0;
    if switch(
        ui,
        SwitchKind::Toggle,
        &mut mono_sum,
        &tr("effects-param-mono-sum"),
    )
    .changed()
    {
        let _ = controller.chain_set_param(row.id, ParamId(2), f32::from(mono_sum));
    }

    let mut phase_invert = row.params[3] != 0.0;
    ui.add_enabled_ui(mono_sum, |ui| {
        if switch(
            ui,
            SwitchKind::Toggle,
            &mut phase_invert,
            &tr("effects-param-phase-invert"),
        )
        .changed()
        {
            let _ = controller.chain_set_param(row.id, ParamId(3), f32::from(phase_invert));
        }
    });

    let mut channel_swap = row.params[4] != 0.0;
    if switch(
        ui,
        SwitchKind::Toggle,
        &mut channel_swap,
        &tr("effects-param-channel-swap"),
    )
    .changed()
    {
        let _ = controller.chain_set_param(row.id, ParamId(4), f32::from(channel_swap));
    }
}

/// The `Performance`/`Quality` mode combo shared by pitch shift and time
/// stretch (contracts/ui-effect-chain.md §3: "Mode combos call
/// `chain_set_mode` — clears the auto-switched flag", FR-008 rule 3).
fn show_mode_combo<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    row: &NodeRow,
    value: f32,
) {
    let current = if value >= 0.5 {
        QualityMode::Quality
    } else {
        QualityMode::Performance
    };
    let mut selected = current;
    ui.label(tr("effects-param-mode"));
    ComboBox::from_id_salt(("effects-mode", row.id.as_u32()))
        .selected_text(tr(mode_label_key(current)))
        .show_ui(ui, |ui| {
            for option in [QualityMode::Performance, QualityMode::Quality] {
                ui.selectable_value(&mut selected, option, tr(mode_label_key(option)));
            }
        });
    if selected != current {
        let _ = controller.chain_set_mode(row.id, selected);
    }
}

const fn mode_label_key(mode: QualityMode) -> &'static str {
    match mode {
        QualityMode::Performance => "effects-mode-performance",
        QualityMode::Quality => "effects-mode-quality",
    }
}

/// The "Add node…" control (contracts/ui-effect-chain.md §2): a combo of
/// [`ADDABLE_KINDS`] plus an "Add" button; refuses inline whenever the
/// chain is at `capacity` (equivalent to the `add` call's own
/// `ChainError::Full`, US2 AS5/SC-007).
fn show_add_row<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    node_count: usize,
    capacity: usize,
) {
    let mut kind = ui
        .ctx()
        .memory(|memory| memory.data.get_temp::<NodeKind>(add_kind_memory_id()))
        .unwrap_or(NodeKind::Gain);

    ui.horizontal(|ui| {
        ComboBox::from_id_salt("effects-add-kind")
            .selected_text(tr(kind.label_key()))
            .show_ui(ui, |ui| {
                for candidate in ADDABLE_KINDS {
                    ui.selectable_value(&mut kind, candidate, tr(candidate.label_key()));
                }
            });
        ui.label(tr("effects-add-node"));

        if ui.button(tr("effects-add")).clicked() {
            let _ = controller.chain_add_node(kind);
        }
    });

    ui.ctx()
        .memory_mut(|memory| memory.data.insert_temp(add_kind_memory_id(), kind));

    if node_count >= capacity {
        ui.label(tr("effects-chain-full"));
    }
}

// -----------------------------------------------------------------------
// 024-effect-chain-rows-and-meters, Phase 5 (US3, contracts/ui-effect-
// chain-rows.md R4, data-model.md §3, research R9): the zero-node chain's
// collapsed/revealed empty state, replacing the bare kind dropdown.
// -----------------------------------------------------------------------

/// Whether the empty state's primary button has been activated, revealing
/// the add row (data-model.md §3). Per-viewer egui temp memory, cleared
/// once the chain has a node (R4.5) so a later empty chain starts
/// collapsed again.
fn empty_add_revealed_id() -> Id {
    Id::new("now-playing-effect-chain-add-revealed")
}

/// One-shot: set alongside the revealed flag, tells the very next frame to
/// request focus on the newly revealed kind combo (data-model.md §3, R4.3),
/// then cleared.
fn empty_add_focus_pending_id() -> Id {
    Id::new("now-playing-effect-chain-add-focus-pending")
}

/// Clears both empty-state flags (R4.5). Idempotent — safe to call every
/// frame the chain is non-empty, whether or not they were ever set.
fn clear_empty_add_state(ui: &Ui) {
    ui.memory_mut(|memory| {
        memory.data.remove::<bool>(empty_add_revealed_id());
        memory.data.remove::<bool>(empty_add_focus_pending_id());
    });
}

/// The zero-node chain's empty state (contract R4): collapsed shows an
/// explanation and one `Primary` "Add effect node" button, no kind combo
/// (R4.2). Activating that button (click/Enter/Space — `button`'s
/// `Response::clicked()` already covers keyboard activation of a focused
/// widget, egui's own `Context::interact`) reveals, from the next frame,
/// the existing add row with its confirm drawn `Primary` instead (R4.3).
/// Exactly one `Primary` control ever shows at once (R4.4).
fn show_empty_state<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
) {
    ui.add(Label::new(tr("effects-empty-explanation")).wrap());

    let revealed = ui
        .memory(|memory| memory.data.get_temp::<bool>(empty_add_revealed_id()))
        .unwrap_or(false);

    if revealed {
        show_revealed_add_row(ui, controller);
    } else if button(ui, Variant::Primary, tr("effects-empty-add")).clicked() {
        ui.memory_mut(|memory| {
            memory.data.insert_temp(empty_add_revealed_id(), true);
            memory.data.insert_temp(empty_add_focus_pending_id(), true);
        });
    }
}

/// The empty state's revealed sub-state (contract R4.3): the same kind
/// combo [`show_add_row`] draws, plus `effects-add-node` and a confirm
/// button — here drawn as the panel's primary action (R4.4's "exactly one
/// Primary"), unlike the ordinary ≥1-node add row's default-styled
/// confirm. Requests
/// focus on the combo exactly once, the frame right after the empty-state
/// primary was activated (`…-focus-pending`, cleared right after).
fn show_revealed_add_row<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
) {
    let mut kind = ui
        .ctx()
        .memory(|memory| memory.data.get_temp::<NodeKind>(add_kind_memory_id()))
        .unwrap_or(NodeKind::Gain);
    let focus_pending = ui
        .memory(|memory| memory.data.get_temp::<bool>(empty_add_focus_pending_id()))
        .unwrap_or(false);

    ui.horizontal(|ui| {
        let combo_response = ComboBox::from_id_salt("effects-add-kind")
            .selected_text(tr(kind.label_key()))
            .show_ui(ui, |ui| {
                for candidate in ADDABLE_KINDS {
                    ui.selectable_value(&mut kind, candidate, tr(candidate.label_key()));
                }
            })
            .response;
        if focus_pending {
            combo_response.request_focus();
        }
        ui.label(tr("effects-add-node"));

        if button(ui, Variant::Primary, tr("effects-add")).clicked() {
            let _ = controller.chain_add_node(kind);
        }
    });

    if focus_pending {
        ui.memory_mut(|memory| memory.data.remove::<bool>(empty_add_focus_pending_id()));
    }

    ui.ctx()
        .memory_mut(|memory| memory.data.insert_temp(add_kind_memory_id(), kind));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// data-model.md §4.1, contract R1.3: on a row's very first frame
    /// nothing has been measured yet, so Parameters (zone 2) falls back
    /// to starting its own line — today's `end_row()` behaviour — while
    /// every other zone's fit test assumes zero width and so never
    /// forces a break on its own.
    #[test]
    fn zone_breaks_first_frame_falls_back_to_parameters_break() {
        assert_eq!(
            zone_breaks(500.0, [None, None, None, None], 8.0),
            [false, false, true, false]
        );
    }

    /// R1.2: a zone breaks exactly when adding it (plus the inter-zone
    /// gap) to the running line width would overflow `available`.
    #[test]
    fn zone_breaks_fit_rule_breaks_when_line_overflows() {
        let widths = [Some(80.0), Some(80.0), Some(300.0), Some(80.0)];
        assert_eq!(
            zone_breaks(300.0, widths, 8.0),
            [false, false, true, true],
            "Parameters (300 wide) can't share row 0's 168 px line within 300 px available, \
             and Actions can't share Parameters' own 300 px line either"
        );
    }

    /// R1.2: when every zone's measured width fits, in order, within one
    /// line, none of them breaks.
    #[test]
    fn zone_breaks_fits_one_line_when_narrow_enough() {
        let widths = [Some(50.0); 4];
        assert_eq!(
            zone_breaks(500.0, widths, 8.0),
            [false, false, false, false]
        );
    }

    /// R1.2 invariant: Identity (zone 0) never breaks, however wide it
    /// measured last frame — it always opens the row's first line.
    #[test]
    fn zone_breaks_zone_zero_never_breaks() {
        let widths = [Some(1000.0), Some(10.0), Some(10.0), Some(10.0)];
        let breaks = zone_breaks(50.0, widths, 8.0);
        assert!(
            !breaks[ZONE_IDENTITY],
            "Identity must never break: {breaks:?}"
        );
        // Every zone after the oversized Identity zone must break too,
        // since Identity alone already exceeds `available`.
        assert_eq!(breaks, [false, true, false, false]);
    }

    /// data-model.md §4.1: an unmeasured non-Parameters zone (`None`) is
    /// treated as zero width by the fit test, so it neither forces a
    /// break nor grows the running line width for the zone after it.
    #[test]
    fn zone_breaks_unmeasured_non_parameters_zone_assumes_zero_width() {
        let widths = [Some(50.0), None, Some(300.0), None];
        assert_eq!(
            zone_breaks(200.0, widths, 8.0),
            [false, false, true, false],
            "State's unmeasured width must not itself force a break, and Parameters must \
             still break purely from its own 300 px overflowing the 200 px available"
        );
    }
}

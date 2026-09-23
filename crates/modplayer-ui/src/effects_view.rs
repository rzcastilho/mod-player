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
//! "Add node…" selection is kept in egui temp memory (like
//! [`panel_open_id`]) rather than an app-owned state struct, so no other
//! screen needs to thread a new field through to reach this one.

use egui::{ComboBox, DragValue, Id, Key, Modifiers, Slider, Ui};
use modplayer_audio_io::OutputBackend;
use modplayer_audio_source::SourceHost;
use modplayer_core::{ChainView, MeterSnapshot, NodeId, NodeRow, PlaybackController, tr, tr_args};
use modplayer_effects::catalog::{NodeKind, NodeOwner, ParamId, QualityMode};

use crate::actions::{self, Claim};
use crate::theme;
use crate::theme::controls::Variant;
use crate::widgets::chain_meters;
use crate::widgets::controls::{SwitchKind, button, destructive_gap, switch};

/// The kinds the "Add node…" control offers (contracts/ui-effect-
/// chain.md §2: "a `ComboBox` of the six kinds").
const ADDABLE_KINDS: [NodeKind; 6] = NodeKind::ALL;

/// Persists the Effect Chain panel's open/closed state across frames in
/// egui's own per-viewer memory (mirrors `now_playing.rs`'s
/// `queue_panel_open_id`).
pub fn panel_open_id() -> Id {
    Id::new("now-playing-effect-chain-open")
}

/// `E` (007's `HostAction::ToggleEffectChain`, contracts/ui-effect-chain.md
/// §1): flips the same egui temp-memory flag the header's "Effects" toggle
/// button reads/writes, so a keyboard toggle and a click stay in sync.
pub fn toggle_effect_chain_panel(ctx: &egui::Context) {
    let id = panel_open_id();
    ctx.memory_mut(|memory| {
        let open = memory.data.get_temp::<bool>(id).unwrap_or(false);
        memory.data.insert_temp(id, !open);
    });
}

/// The egui memory id the "Add node…" combo's own selection persists
/// under (a per-viewer convenience, like `panel_open_id`).
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
/// 0 % figures is valid).
pub fn show<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
) {
    let view = controller.chain_view();
    let meters = controller.chain_meters();

    show_header(ui, &view, &meters);

    for row in &view.nodes {
        show_row(ui, controller, row);
    }

    show_add_row(ui, controller, view.nodes.len(), view.capacity);
}

/// The panel header (contracts/ui-effect-chain.md §2): title, whole-chain
/// CPU figure, overload counter, an over-budget badge while `over_budget`,
/// the pre-/post-chain level pairs and the 64-band spectrum — all read
/// live from `RtShared` via `ChainView`/`MeterSnapshot` every frame.
fn show_header(ui: &mut Ui, view: &ChainView, meters: &MeterSnapshot) {
    ui.horizontal(|ui| {
        // 014-design-tokens-and-type-scale (US2, T028, data-model.md §6:
        // "Effect chain" is the named `section`-role example) — the
        // accessible name is pinned back to the exact, un-uppercased
        // title (FR-019), mirroring `markers::panel`'s T030 and
        // `settings::controls::section_heading`.
        let title = tr("effects-panel-title");
        let heading = ui.label(theme::section_label(&title));
        ui.ctx().accesskit_node_builder(heading.id, |b| {
            b.set_label(title.clone());
        });
        ui.label(tr_args(
            "effects-chain-cpu",
            &[("pct", format!("{:.0}", view.total_cost_pct))],
        ));
        ui.label(tr_args(
            "effects-overloads",
            &[("count", view.overload_count.to_string())],
        ));
        if view.over_budget {
            ui.label(tr("effects-over-budget-badge"));
        }
    });
    ui.horizontal(|ui| {
        chain_meters::level_pair(ui, "effects-pre", meters.pre);
        chain_meters::level_pair(ui, "effects-post", meters.post);
    });
    chain_meters::spectrum(ui, &meters.spectrum);
}

/// One node's row (contracts/ui-effect-chain.md §2/§4): a `dnd_drop_zone`
/// wrapping a `ui.horizontal` of the drag handle, kind/owner labels, the
/// bypass toggle, the CPU figure, this kind's parameter controls, the
/// "quality mode auto-switched" note (if any), and the remove button. A
/// pointer drop of another row's handle onto this one reorders via
/// `chain_move_node`.
fn show_row<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    row: &NodeRow,
) {
    let (_frame, payload) = ui.dnd_drop_zone::<NodeId, _>(egui::Frame::group(ui.style()), |ui| {
        ui.horizontal(|ui| {
            show_handle(ui, row);

            ui.label(format!("{}. {}", row.index + 1, tr(row.kind.label_key())));
            ui.label(tr(owner_label_key(row.owner)));

            let mut bypassed = row.bypassed;
            if switch(ui, SwitchKind::Toggle, &mut bypassed, &tr("effects-bypass")).changed() {
                let _ = controller.chain_set_bypass(row.id, bypassed);
            }
            if row.auto_bypassed {
                // 014-design-tokens-and-type-scale (US2, T028): a
                // supplementary status note, `secondary`/`.weak()` like a
                // row's other detail text.
                ui.label(egui::RichText::new(tr("effects-auto-bypassed")).weak());
            }

            ui.label(tr_args(
                "effects-cpu",
                &[("pct", format!("{:.0}", row.cost_pct))],
            ));

            show_params(ui, controller, row);

            if row.mode_note {
                ui.label(egui::RichText::new(tr("effects-mode-note")).weak());
            }

            destructive_gap(ui);
            if button(ui, Variant::Destructive, tr("effects-remove")).clicked() {
                let _ = controller.chain_remove_node(row.id);
            }
        });
    });

    if let Some(dragged) = payload
        && *dragged != row.id
    {
        let _ = controller.chain_move_node(*dragged, row.index);
    }
}

/// The drag handle (contracts/ui-effect-chain.md §4): a focusable "⋮"
/// that starts a pointer drag carrying this node's [`NodeId`] as the DnD
/// payload, and claims `↑`/`↓` (`effect_handle_claims`,
/// `handle_focused_handle_keys` applies them) so the dispatcher leaves
/// them for this widget while it has focus.
fn show_handle(ui: &mut Ui, row: &NodeRow) {
    let id = handle_id(row.id);
    let response = ui
        .dnd_drag_source(id, row.id, |ui| {
            ui.label("⋮");
        })
        .response;
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tr("effects-reorder-handle"))
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

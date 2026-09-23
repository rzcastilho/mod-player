// SPDX-License-Identifier: MIT OR Apache-2.0

//! The plugin panel host (011-plugin-ui-contributions US1,
//! contracts/ui-panels.md): renders every currently visible panel —
//! docked in a fixed-width column at the right edge of Now Playing's
//! content (L1), floated in its own `egui::Window` (L2) — with every
//! widget kind fully keyboard-operable and AccessKit-labelled (§3), and
//! zero plugin code executed while drawing (Constitution III). The sole
//! read model is `PlaybackController::plugin_panels_view`; every
//! interaction round-trips through one of the controller's own
//! `plugin_panel_*` methods (§1 P4, §2 L3/L5/L6). No colour literal
//! appears anywhere here (A4): every stroke/fill comes from
//! `ui.visuals()`/`ui.style()`.

use egui::{Context, Id, Key, Rect, ScrollArea, Sense, Slider, Ui, Vec2, Window};
use modplayer_audio_io::OutputBackend;
use modplayer_audio_source::SourceHost;
use modplayer_capability_gateway::ui::{UiId, WidgetKind, WidgetValue};
use modplayer_core::actions::{KeyName, Mods};
use modplayer_core::plugins::ui::panel::{PanelKey, WidgetState};
use modplayer_core::plugins::{PanelBody, PanelView};
use modplayer_core::settings::PanelPlacement;
use modplayer_core::{PlaybackController, PluginId, tr, tr_args};
use modplayer_plugin_runtime::events::SuspendCause;

use crate::actions::{self, ChordPattern, Claim};
use crate::plugin_assets;
use crate::theme;
use crate::theme::controls::Variant;
use crate::widgets::controls::{SwitchKind, button, destructive_gap, switch};
use crate::widgets::knob;

/// L1: the docked column's fixed width.
pub const DOCK_WIDTH: f32 = 280.0;
/// L2: a floated panel's minimum size.
const FLOAT_MIN: Vec2 = Vec2::new(200.0, 120.0);
/// L2: a floated panel's default size — "docked size" (a plausible
/// column-shaped default; the user's own resize is what persists from
/// here on, L3).
const FLOAT_DEFAULT_SIZE: Vec2 = Vec2::new(DOCK_WIDTH, 320.0);
/// The `knob` kind's fixed square size.
const KNOB_SIZE: f32 = 40.0;
/// The header/list icon size (L5).
const ICON_SIZE: f32 = 16.0;
/// PgUp/PgDn's step multiplier over a slider/knob's own `step` (mirrors
/// `widgets::volume`'s `PAGE_STEP`).
const PAGE_STEPS: f64 = 10.0;

fn plain(k: KeyName) -> ChordPattern {
    (Mods::default(), k)
}

fn shift(k: KeyName) -> ChordPattern {
    (
        Mods {
            shift: true,
            ..Mods::default()
        },
        k,
    )
}

fn key(name: &str) -> KeyName {
    KeyName::parse(name).unwrap_or_else(|| unreachable!("{name:?} is not in KEY_NAMES"))
}

/// `slider`/`knob` (contracts/ui-panels.md §3): `←/→/↑/↓` step, PgUp/PgDn
/// page-step.
fn numeric_claims() -> Vec<ChordPattern> {
    vec![
        plain(key("Left")),
        plain(key("Right")),
        plain(key("Up")),
        plain(key("Down")),
        plain(key("PageUp")),
        plain(key("PageDown")),
        plain(key("Tab")),
        shift(key("Tab")),
    ]
}

/// `list` (contracts/ui-panels.md §3): `↑/↓`, `Home/End`.
fn list_claims() -> Vec<ChordPattern> {
    vec![
        plain(key("Up")),
        plain(key("Down")),
        plain(key("Home")),
        plain(key("End")),
        plain(key("Tab")),
        shift(key("Tab")),
    ]
}

/// L1: the docked column, only while at least one docked panel is
/// visible. `egui::Panel` (0.36's unified side/top/bottom panel) is safe
/// to nest inside the already-open `CentralPanel`/Now Playing content
/// `Ui` — `app.rs`'s own nav rail (`Panel::left`) already relies on
/// exactly this — but, like that nav rail, it must be the *first* thing
/// drawn into that `Ui` so the rest of Now Playing's content correctly
/// sees the narrower remaining width; `now_playing.rs` calls this before
/// any of its own content (T055).
pub fn show_dock<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
) {
    let view = controller.plugin_panels_view();
    if view.docked.is_empty() {
        return;
    }
    egui::Panel::right(Id::new("plugin-panel-dock"))
        .exact_size(DOCK_WIDTH)
        .resizable(false)
        .show(ui, |ui| {
            ScrollArea::vertical()
                .id_salt("plugin-panel-dock-scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for panel in &view.docked {
                        ui.group(|ui| show_panel(ui, controller, panel));
                    }
                });
        });
}

/// L2: every currently floated panel, each its own `egui::Window` — a
/// position-independent overlay, so unlike [`show_dock`] it can be called
/// from anywhere in the frame (`now_playing.rs` calls it after its own
/// content, T055).
pub fn show_floated_windows<B: OutputBackend, H: SourceHost>(
    ctx: &Context,
    controller: &mut PlaybackController<B, H>,
) {
    let view = controller.plugin_panels_view();
    for panel in &view.floated {
        show_floated(ctx, controller, panel);
    }
}

fn window_id(key: &PanelKey) -> Id {
    Id::new((
        "plugin-panel-window",
        key.plugin.as_str(),
        key.panel.as_str(),
    ))
}

/// L2/L4: `persisted`'s x/y/w/h clamped inside `available` (the Now
/// Playing content rect) — a pure function so both a restored geometry
/// and the very first float (no geometry yet) run through one path.
fn clamp_into(available: Rect, pos: egui::Pos2, size: Vec2) -> (egui::Pos2, Vec2) {
    let size = Vec2::new(
        size.x
            .clamp(FLOAT_MIN.x, available.width().max(FLOAT_MIN.x)),
        size.y
            .clamp(FLOAT_MIN.y, available.height().max(FLOAT_MIN.y)),
    );
    let max_x = (available.right() - size.x).max(available.left());
    let max_y = (available.bottom() - size.y).max(available.top());
    let pos = egui::pos2(
        pos.x.clamp(available.left(), max_x),
        pos.y.clamp(available.top(), max_y),
    );
    (pos, size)
}

fn show_floated<B: OutputBackend, H: SourceHost>(
    ctx: &Context,
    controller: &mut PlaybackController<B, H>,
    panel: &PanelView,
) {
    let available = ctx.input(|i| i.content_rect());
    let geometry = panel.geometry;
    let default_pos = geometry
        .and_then(|g| Some(egui::pos2(g.x?, g.y?)))
        .unwrap_or_else(|| available.center() - FLOAT_DEFAULT_SIZE / 2.0);
    let default_size = geometry
        .and_then(|g| Some(Vec2::new(g.w?, g.h?)))
        .unwrap_or(FLOAT_DEFAULT_SIZE);
    let (pos, size) = clamp_into(available, default_pos, default_size);

    let title = tr_args(
        "plugin-panel-header",
        &[
            ("plugin", panel.plugin_name.clone()),
            ("title", panel.title.clone()),
        ],
    );
    let key = panel.key.clone();
    Window::new(title)
        .id(window_id(&key))
        .order(crate::shell::PLUGIN_FLOATED_WINDOW_ORDER)
        .constrain(true)
        .resizable(true)
        .min_size(FLOAT_MIN)
        .default_pos(pos)
        .default_size(size)
        .show(ctx, |ui| {
            show_panel(ui, controller, panel);
        });

    // L3: persist a real move/resize — this only ever fires when the rect
    // actually differs (beyond float noise) from what is already stored,
    // so an untouched window never writes on every frame.
    if let Some(rect) = ctx.memory(|m| m.area_rect(window_id(&key))) {
        const EPSILON: f32 = 0.5;
        let differs =
            |stored: Option<f32>, live: f32| stored.is_none_or(|s| (s - live).abs() > EPSILON);
        let changed = differs(geometry.and_then(|g| g.x), rect.left())
            || differs(geometry.and_then(|g| g.y), rect.top())
            || differs(geometry.and_then(|g| g.w), rect.width())
            || differs(geometry.and_then(|g| g.h), rect.height());
        if changed {
            let mut persisted = geometry.unwrap_or_default();
            persisted.placement = PanelPlacement::Floated;
            persisted.x = Some(rect.left());
            persisted.y = Some(rect.top());
            persisted.w = Some(rect.width());
            persisted.h = Some(rect.height());
            controller.plugin_panel_set_placement(&key, persisted);
        }
    }
}

/// One panel's whole body: header (L5), then either its live widgets or
/// the suspended placeholder (P5).
fn show_panel<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    panel: &PanelView,
) {
    show_header(ui, controller, panel);
    match &panel.body {
        PanelBody::Live(widgets) => {
            for widget in widgets {
                show_widget(ui, controller, panel.plugin, &panel.key.panel, widget);
                eprintln!(
                    "TRACE after widget={} focus={:?}",
                    widget.spec.id.as_str(),
                    ui.memory(|m| m.focused())
                );
            }
        }
        PanelBody::Placeholder { cause } => show_placeholder(ui, controller, panel, *cause),
    }
}

/// L5: plugin icon/generic glyph · plugin name · panel title, then the
/// `[Float|Dock] [Close] [Disable]` controls.
fn show_header<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    panel: &PanelView,
) {
    ui.horizontal(|ui| {
        match controller.plugin_assets(panel.plugin) {
            Some(assets) if panel.has_icon => {
                plugin_assets::show_icon(ui, panel.plugin, assets, ICON_SIZE)
            }
            _ => plugin_assets::generic_glyph(ui, ICON_SIZE),
        }
        // 014-design-tokens-and-type-scale (US2, T034): the chrome header
        // is this panel's own `section`-role heading (data-model.md §6's
        // "Panel/group headers" — the plugin-drawn body below it is out of
        // scope, A9). The accessible name is pinned back to the exact,
        // un-uppercased title (FR-019), mirroring `markers::panel`'s T030.
        let header_text = tr_args(
            "plugin-panel-header",
            &[
                ("plugin", panel.plugin_name.clone()),
                ("title", panel.title.clone()),
            ],
        );
        let header = ui.label(theme::section_label(&header_text));
        ui.ctx().accesskit_node_builder(header.id, |b| {
            b.set_label(header_text.clone());
        });

        let float_key = match panel.placement {
            PanelPlacement::Docked => "plugin-panel-float",
            PanelPlacement::Floated => "plugin-panel-dock",
        };
        if ui.button(tr(float_key)).clicked() {
            let mut persisted = panel.geometry.unwrap_or_default();
            persisted.placement = match panel.placement {
                PanelPlacement::Docked => PanelPlacement::Floated,
                PanelPlacement::Floated => PanelPlacement::Docked,
            };
            controller.plugin_panel_set_placement(&panel.key, persisted);
        }
        if ui.button(tr("plugin-panel-close")).clicked() {
            controller.plugin_panel_close(&panel.key);
        }
        destructive_gap(ui);
        if button(ui, Variant::Destructive, tr("plugin-panel-disable")).clicked() {
            controller.plugin_panel_set_disabled(&panel.key, true);
        }
    });
}

/// P5: a suspended plugin's panel keeps its place but runs no plugin
/// code — the cause plus a Restart action (contracts/ui-plugins.md's own
/// `plugin_restart`, already the notification-action target).
fn show_placeholder<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    panel: &PanelView,
    cause: SuspendCause,
) {
    ui.label(tr_args(
        "plugin-panel-suspended",
        &[
            ("plugin", panel.plugin_name.clone()),
            ("cause", tr(cause_key(cause))),
        ],
    ));
    if ui.button(tr("plugin-panel-restart")).clicked() {
        controller.plugin_restart(panel.plugin);
    }
}

const fn cause_key(cause: SuspendCause) -> &'static str {
    match cause {
        SuspendCause::Hang => "plugin-suspended-cause-hang",
        SuspendCause::CpuShare => "plugin-suspended-cause-cpu-share",
        SuspendCause::Memory => "plugin-suspended-cause-memory",
        SuspendCause::DidNotStart => "plugin-suspended-cause-did-not-start",
    }
}

/// One widget, dispatched by kind (contracts/ui-panels.md §3's table).
fn show_widget<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    plugin: PluginId,
    panel: &UiId,
    widget: &WidgetState,
) {
    let spec = &widget.spec;
    match spec.kind {
        WidgetKind::Label => {
            let text = match &widget.value {
                WidgetValue::Text(t) if !t.is_empty() => t.clone(),
                _ => spec.label.clone(),
            };
            ui.label(text);
        }
        WidgetKind::Text => {
            let text = match &widget.value {
                WidgetValue::Text(t) => t.clone(),
                _ => String::new(),
            };
            ui.add(egui::Label::new(format!("{}: {text}", spec.label)).wrap());
        }
        WidgetKind::Button => {
            if ui.button(&spec.label).clicked() {
                controller.plugin_panel_interaction(
                    plugin,
                    panel,
                    &spec.id,
                    WidgetValue::Bool(true),
                );
            }
        }
        WidgetKind::Toggle => {
            let mut checked = matches!(widget.value, WidgetValue::Bool(true));
            let response = switch(ui, SwitchKind::Checkbox, &mut checked, &spec.label);
            if response.changed() {
                controller.plugin_panel_interaction(
                    plugin,
                    panel,
                    &spec.id,
                    WidgetValue::Bool(checked),
                );
            }
        }
        WidgetKind::Slider | WidgetKind::Knob => {
            show_numeric(
                ui,
                controller,
                plugin,
                panel,
                spec.kind == WidgetKind::Knob,
                widget,
            );
        }
        WidgetKind::List => show_list(ui, controller, plugin, panel, widget),
        WidgetKind::MarkerList => {
            ui.label(&spec.label);
            ui.label(tr("plugin-marker-list-empty"));
        }
        WidgetKind::Meter => show_meter(ui, widget),
    }
}

/// Per-widget drag state (contracts/ui-panels.md §3: a key step commits
/// immediately; a pointer drag commits once, on release).
#[derive(Clone, Copy)]
struct DragState {
    dragging: bool,
    value: f64,
}

fn drag_state_id(plugin: PluginId, panel: &UiId, widget: &UiId) -> Id {
    Id::new((
        "plugin-panel-drag",
        plugin.0,
        panel.as_str(),
        widget.as_str(),
    ))
}

#[allow(clippy::too_many_arguments)]
fn show_numeric<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    plugin: PluginId,
    panel: &UiId,
    is_knob: bool,
    widget: &WidgetState,
) {
    let spec = &widget.spec;
    let current = match widget.value {
        WidgetValue::Number(n) => n,
        _ => 0.0,
    };
    let min = spec.min.unwrap_or(0.0);
    let max = spec.max.unwrap_or(1.0);
    let step = spec.step.unwrap_or(1.0).max(f64::EPSILON);

    let mem_id = drag_state_id(plugin, panel, &spec.id);
    let mut state: DragState = ui.memory(|m| m.data.get_temp(mem_id)).unwrap_or(DragState {
        dragging: false,
        value: current,
    });
    if !state.dragging {
        state.value = current;
    }
    let mut value = state.value;

    ui.horizontal(|ui| {
        // A1: the slider/knob's own accessible name comes from
        // `Slider::text` (`WidgetInfo::slider` reads it, regardless of
        // `show_value`) — no separate sibling `ui.label`, so a reader
        // announces one node ("Tempo, slider, 120"), not two.
        //
        // `.show_value(false)` on the plain-slider branch too (T042's own
        // `arrows_step_and_emit` exposed this): egui's default
        // `show_value(true)` draws the numeric readout as its own small
        // editable sub-widget layered over the slider's tail — every key
        // step that redraws it with a *new* value (as ours always does,
        // since `value` is re-derived from `WidgetState` each frame
        // rather than owned by the widget itself) then silently steals
        // keyboard focus onto that sub-widget after the very first step,
        // leaving every further arrow-key press with nothing focused to
        // act on. The knob already disabled it for its own reason (no
        // room to paint a number over the dial); this fixes the plain
        // slider's own repeated-stepping for the identical reason.
        let response = if is_knob {
            knob::knob(ui, &mut value, min..=max, step, KNOB_SIZE, &spec.label)
        } else {
            ui.add(
                Slider::new(&mut value, min..=max)
                    .step_by(step)
                    .show_value(false)
                    .text(spec.label.clone()),
            )
        };
        actions::register_claim(ui.ctx(), response.id, Claim::Keys(numeric_claims()));
        eprintln!(
            "TRACE widget={} response.id={:?} has_focus={} value_in={value} changed={}",
            spec.id.as_str(),
            response.id,
            response.has_focus(),
            response.changed()
        );

        let mut commit = None;
        if response.drag_started() {
            state.dragging = true;
        }
        if response.dragged() {
            state.dragging = true;
            state.value = value;
        }
        if response.drag_stopped() {
            commit = Some(value);
            state.dragging = false;
            state.value = value;
        } else if response.changed() && !state.dragging {
            commit = Some(value);
            state.value = value;
        }
        if response.has_focus() {
            let (page_up, page_down) =
                ui.input(|i| (i.key_pressed(Key::PageUp), i.key_pressed(Key::PageDown)));
            if page_up {
                value = (value + step * PAGE_STEPS).min(max);
                commit = Some(value);
                state.value = value;
            } else if page_down {
                value = (value - step * PAGE_STEPS).max(min);
                commit = Some(value);
                state.value = value;
            }
        }

        ui.memory_mut(|m| m.data.insert_temp(mem_id, state));
        if let Some(value) = commit {
            controller.plugin_panel_interaction(
                plugin,
                panel,
                &spec.id,
                WidgetValue::Number(value),
            );
        }
    });
}

fn show_list<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    plugin: PluginId,
    panel: &UiId,
    widget: &WidgetState,
) {
    let spec = &widget.spec;
    let (items, selected) = match &widget.value {
        WidgetValue::Items { items, selected } => (items.clone(), selected.clone()),
        _ => (spec.items.clone(), spec.selected.clone()),
    };
    ui.label(&spec.label);
    ScrollArea::vertical()
        .id_salt((
            "plugin-panel-list",
            plugin.0,
            panel.as_str(),
            spec.id.as_str(),
        ))
        .max_height(120.0)
        .show(ui, |ui| {
            for item in &items {
                let is_selected = selected.as_ref() == Some(&item.id);
                let response = ui.selectable_label(is_selected, &item.label);
                actions::register_claim(ui.ctx(), response.id, Claim::Keys(list_claims()));
                if response.clicked() && !is_selected {
                    controller.plugin_panel_interaction(
                        plugin,
                        panel,
                        &spec.id,
                        WidgetValue::Items {
                            items: items.clone(),
                            selected: Some(item.id.clone()),
                        },
                    );
                }
            }
        });
}

fn show_meter(ui: &mut Ui, widget: &WidgetState) {
    let spec = &widget.spec;
    let fraction = match widget.value {
        WidgetValue::Number(n) => n.clamp(0.0, 1.0),
        _ => 0.0,
    };
    ui.horizontal(|ui| {
        ui.label(&spec.label);
        let size = Vec2::new(ui.available_width().clamp(60.0, 200.0), 12.0);
        let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            painter.rect_filled(rect, 2.0, ui.visuals().extreme_bg_color);
            let mut fill = rect;
            fill.set_width(rect.width() * fraction as f32);
            painter.rect_filled(fill, 2.0, ui.visuals().selection.bg_fill);
        }
        let pct = (fraction * 100.0).round();
        let value_text = format!("{}: {pct}%", spec.label);
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::ProgressIndicator,
                true,
                value_text.clone(),
            )
        });
    });
}

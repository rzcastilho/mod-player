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

use egui::{
    Area, Button, Context, CursorIcon, Id, Key, Label, Rect, ScrollArea, Sense, Slider,
    TextWrapMode, Ui, Vec2, Window, accesskit,
};
use modplayer_audio_io::OutputBackend;
use modplayer_audio_source::SourceHost;
use modplayer_capability_gateway::ui::{UiId, WidgetKind, WidgetValue};
use modplayer_core::actions::{KeyName, Mods};
use modplayer_core::plugins::ui::panel::{PanelKey, WidgetState};
use modplayer_core::plugins::{PanelBody, PanelView};
use modplayer_core::settings::{
    DOCK_WIDTH_DEFAULT, DOCK_WIDTH_MAX, DOCK_WIDTH_MIN, PanelPlacement,
};
use modplayer_core::{PlaybackController, PluginId, tr, tr_args};
use modplayer_plugin_runtime::events::SuspendCause;

use crate::actions::{self, ChordPattern, Claim};
use crate::layout;
use crate::plugin_assets;
use crate::theme;
use crate::theme::controls::Variant;
use crate::widgets::controls::{SwitchKind, button, destructive_gap, switch};
use crate::widgets::knob;

/// L2: a floated panel's minimum size.
const FLOAT_MIN: Vec2 = Vec2::new(200.0, 120.0);
/// L2: a floated panel's default size — "docked size" (a plausible
/// column-shaped default; the user's own resize is what persists from
/// here on, L3). `DOCK_WIDTH_DEFAULT` (018-window-sizing-and-responsive-
/// dock): a float's default size no longer tracks the dock's own
/// (now variable) render width, just its persisted default.
const FLOAT_DEFAULT_SIZE: Vec2 = Vec2::new(DOCK_WIDTH_DEFAULT, 320.0);
/// The `knob` kind's fixed square size.
const KNOB_SIZE: f32 = 40.0;
/// The header/list icon size (L5).
const ICON_SIZE: f32 = 16.0;
/// The header's minimum title budget (018-window-sizing-and-responsive-
/// dock, contract D7, research R9): below this, the title and buttons
/// split onto their own rows instead of sharing one. `~64 pt`, enough for
/// ~6 glyphs plus "…" — a `theme::space`-derived token, not a bare
/// literal.
const TITLE_MIN_WIDTH: f32 = 2.0 * theme::space::XXL;
/// PgUp/PgDn's step multiplier over a slider/knob's own `step` (mirrors
/// `widgets::volume`'s `PAGE_STEP`).
const PAGE_STEPS: f64 = 10.0;
/// 018-window-sizing-and-responsive-dock (contract D5/D6): the splitter's
/// own hit rect straddles its region's left edge by half its width
/// (`HIT_WIDTH` in `show_dock_splitter`), so a small margin absorbs that
/// when checking whether the focused widget's rect falls inside a
/// remembered dock/overlay region.
const FOCUS_ZONE_MARGIN: f32 = 8.0;

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
    // 018-window-sizing-and-responsive-dock (contract D2/D3, research
    // R6/R7): `content_rect` is this `Ui`'s own rect, read before anything
    // else draws into it (`now_playing.rs` already calls `show_dock` first,
    // L1) — the splitter needs it to place its hit rect, and
    // `layout::effective_dock_width` needs its width.
    let content_rect = ui.max_rect();
    let stored = controller.window_settings().dock_width;
    let effective = show_dock_splitter(ui, controller, content_rect, stored);
    let panel = egui::Panel::right(Id::new("plugin-panel-dock"))
        .exact_size(effective)
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
    // 018-window-sizing-and-responsive-dock (contract D6, data-model.md
    // §3's focus rule): remember where the dock drew this frame, so a
    // later frame that stops drawing it at all (window narrows below
    // 1024) can still tell whether the widget that was focused belonged
    // to it.
    remember_dock_region(ui.ctx(), panel.response.rect);
}

/// D1 (018-window-sizing-and-responsive-dock, data-model.md §3): the
/// session-only flag for whether the narrow-window overlay is open — egui
/// temp memory, read/written only through [`overlay_open`]/
/// [`set_overlay_open`], never persisted (contract D5).
fn overlay_open_key() -> Id {
    Id::new("plugin-dock-overlay-open")
}

/// Whether the narrow-window overlay is currently open (data-model.md
/// §3).
pub fn overlay_open(ctx: &Context) -> bool {
    ctx.memory(|m| m.data.get_temp(overlay_open_key()))
        .unwrap_or(false)
}

/// Set the narrow-window overlay's open flag (contract D4's toggle, D5's
/// dismissal paths) — session-only, never written to `WindowSettings`.
pub fn set_overlay_open(ctx: &Context, open: bool) {
    ctx.memory_mut(|m| m.data.insert_temp(overlay_open_key(), open));
}

/// D1: this frame's dock presentation (data-model.md §3 truth table) —
/// `window_width` is the *whole window's* own content rect (`ctx.
/// content_rect()`, distinct from the *Now Playing* content rect
/// `show_dock`/`show_overlay` use for `effective_dock_width`), `docked_
/// count` the number of currently docked panels, and the overlay-open
/// flag above. Force-clears that flag whenever the result is `Docked`/
/// `None` (data-model.md §3's transition table: widening past 1024 or the
/// last docked panel closing/floating both close the overlay too), so a
/// stale `true` never resurfaces as a phantom `Overlay` the next time the
/// window narrows again.
pub fn dock_presentation<B: OutputBackend, H: SourceHost>(
    ctx: &Context,
    controller: &PlaybackController<B, H>,
) -> layout::DockPresentation {
    let window_width = ctx.content_rect().width();
    let docked_count = controller.plugin_panels_view().docked.len();
    let presentation = layout::dock_presentation(window_width, docked_count, overlay_open(ctx));
    if matches!(
        presentation,
        layout::DockPresentation::Docked | layout::DockPresentation::None
    ) {
        set_overlay_open(ctx, false);
    }
    presentation
}

/// D6/data-model.md §3's focus rule: where the dock/overlay last actually
/// drew, remembered across frames — a frame whose presentation is
/// `Hidden`/`None` draws neither at all, so it has nothing of its own to
/// ask; [`continue_focus_past_dock`] reads whatever the last frame that
/// *did* draw one left here.
fn dock_region_key() -> Id {
    Id::new("plugin-dock-region")
}

fn remember_dock_region(ctx: &Context, rect: Rect) {
    ctx.memory_mut(|m| m.data.insert_temp(dock_region_key(), rect));
}

fn dock_region(ctx: &Context) -> Option<Rect> {
    ctx.memory(|m| m.data.get_temp(dock_region_key()))
}

/// The currently focused widget's id and its own last-drawn rect
/// (`Context::read_response`, exactly as `widgets::controls::
/// paint_focus_ring` already reads it), if any.
fn focused_response(ctx: &Context) -> Option<(Id, Rect)> {
    let id = ctx.memory(|m| m.focused())?;
    let rect = ctx.read_response(id)?.rect;
    Some((id, rect))
}

/// D6/data-model.md §3's focus rule: if the widget focused coming into
/// this frame belonged to the dock/overlay the last time either actually
/// drew, and `presentation` draws neither this frame, move focus onward —
/// returns `true` when the caller (`now_playing.rs`) should focus its
/// "Panels" toggle once it draws it later this same frame (`Hidden`), or
/// surrenders focus outright when there is no toggle to receive it
/// (`None`). A no-op (returns `false`) whenever `presentation` still draws
/// something (`Docked`/`Overlay` — Docked ↔ Overlay keeps the same widget
/// focused for free, since both draw the same widget ids) or nothing was
/// focused/remembered to begin with.
pub fn continue_focus_past_dock(ctx: &Context, presentation: layout::DockPresentation) -> bool {
    if !matches!(
        presentation,
        layout::DockPresentation::Hidden | layout::DockPresentation::None
    ) {
        return false;
    }
    let Some(region) = dock_region(ctx) else {
        return false;
    };
    let Some((id, rect)) = focused_response(ctx) else {
        return false;
    };
    if !region.expand(FOCUS_ZONE_MARGIN).contains(rect.center()) {
        return false;
    }
    if presentation == layout::DockPresentation::Hidden {
        true
    } else {
        ctx.memory_mut(|m| m.surrender_focus(id));
        false
    }
}

/// D5: while the focused widget's rect falls inside `region`, keep its
/// focus through `Escape` — egui's own default (`EventFilter::escape` is
/// `false` unless a widget opts in) is to clear focus on any *unclaimed*
/// `Escape` press during `Context::run`'s own `begin_pass`, **before**
/// this file's own draw code ever runs, which would otherwise erase the
/// very focus [`show_overlay`]'s own Escape check depends on. Re-armed
/// every frame focus is still inside, so the lock is already in effect by
/// the time `Escape` is actually pressed (`Memory::set_focus_lock_filter`
/// requires the widget to already have had focus on a *previous* frame).
fn hold_focus_through_escape(ctx: &Context, region: Rect) {
    if let Some((id, rect)) = focused_response(ctx)
        && region.expand(FOCUS_ZONE_MARGIN).contains(rect.center())
    {
        ctx.memory_mut(|m| {
            m.set_focus_lock_filter(
                id,
                egui::EventFilter {
                    escape: true,
                    // The filter replaces rather than merges, so keep the
                    // splitter's own ←/→ lock (`show_dock_splitter`) too.
                    horizontal_arrows: id == splitter_id(),
                    ..Default::default()
                },
            );
        });
    }
}

/// D1/D2/D3/D5: the narrow-window overlay — the same splitter
/// ([`show_dock_splitter`]) and docked-panel content as [`show_dock`],
/// drawn as a right-anchored `egui::Area` over `content_rect` (the Now
/// Playing content `Ui`'s own rect, captured by the caller *before*
/// anything else draws into it — exactly what `show_dock` itself reads via
/// `ui.max_rect()` had it run instead) rather than a `Panel::right`
/// column. Only called while `dock_presentation` resolves to `Overlay`.
/// Returns whether an in-overlay `Escape` closed it this frame (contract
/// D5) — the caller then focuses its "Panels" toggle once it draws it,
/// later this same frame (mirrors [`continue_focus_past_dock`]'s own
/// `Hidden` case).
pub fn show_overlay<B: OutputBackend, H: SourceHost>(
    ctx: &Context,
    controller: &mut PlaybackController<B, H>,
    content_rect: Rect,
) -> bool {
    let view = controller.plugin_panels_view();
    if view.docked.is_empty() {
        return false;
    }
    let stored = controller.window_settings().dock_width;
    let mut escape_closed = false;

    Area::new(Id::new("plugin-dock-overlay"))
        .order(crate::shell::PLUGIN_DOCK_OVERLAY_ORDER)
        .fixed_pos(content_rect.min)
        .show(ctx, |ui| {
            let effective = show_dock_splitter(ui, controller, content_rect, stored);
            let rect = Rect::from_min_size(
                egui::pos2(content_rect.right() - effective, content_rect.top()),
                Vec2::new(effective, content_rect.height()),
            );
            if ui.is_rect_visible(rect) {
                ui.painter().rect_filled(rect, 0.0, ui.visuals().panel_fill);
            }
            ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                ScrollArea::vertical()
                    .id_salt("plugin-panel-overlay-scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for panel in &view.docked {
                            ui.group(|ui| show_panel(ui, controller, panel));
                        }
                    });
            });

            // D5: keep whatever is focused focused through `Escape`
            // (`hold_focus_through_escape`, above) before checking for
            // one — otherwise egui's own default already cleared it
            // before this closure ever ran, and the check below would
            // never see it. `Escape` then closes the overlay only while
            // focus is inside it — `splitter_claims` includes `Escape` so
            // a focused splitter's own narrowed claim doesn't swallow the
            // event before this check ever sees it; every other header/
            // body widget in here already defaults to owning `Escape`
            // itself (contracts/ui-actions.md's toolkit-default claim),
            // so either way the key never reaches `actions::dispatch`'s
            // global registry.
            hold_focus_through_escape(ui.ctx(), rect);
            if ui.input(|i| i.key_pressed(Key::Escape))
                && focused_response(ui.ctx()).is_some_and(|(_, focused_rect)| {
                    rect.expand(FOCUS_ZONE_MARGIN)
                        .contains(focused_rect.center())
                })
            {
                escape_closed = true;
            }

            remember_dock_region(ui.ctx(), rect);
        });

    if escape_closed {
        set_overlay_open(ctx, false);
    }
    escape_closed
}

/// D2: the width the docked column/overlay is drawn at this frame — the
/// splitter's in-progress drag width if one is live, else the stored
/// width — clamped by `layout::effective_dock_width` against
/// `content_width`. Lets content beside the dock (the transport row) wrap
/// at the dock's actual edge.
pub fn live_dock_width<B: OutputBackend, H: SourceHost>(
    ctx: &Context,
    controller: &PlaybackController<B, H>,
    content_width: f32,
) -> f32 {
    let dragging: Option<f32> = ctx.memory(|m| m.data.get_temp(splitter_id()));
    layout::effective_dock_width(
        dragging.unwrap_or(controller.window_settings().dock_width),
        content_width,
    )
}

/// D3: the fixed `Id` the dock/overlay splitter's in-progress drag width
/// lives under in egui temp memory while a drag is active — a render-time
/// value only; `controller.set_dock_width` (called on `drag_stopped()` or
/// a keyboard step) is the only thing that ever persists a width.
fn splitter_id() -> Id {
    Id::new("plugin-panel-dock-splitter")
}

/// D3: the splitter's own key claims — `←/→/Home/End` step/jump the width
/// (data-model.md §4) — plus `Tab`/`Shift+Tab`, mirroring every other
/// claim set in this file (e.g. [`numeric_claims`]) even though egui's own
/// dispatcher rule already leaves `Tab` to the toolkit regardless.
/// `Escape` is explicit here too (018-window-sizing-and-responsive-dock,
/// contract D5): a focused splitter's own claim would otherwise narrow to
/// exactly these chords and stop "owning" `Escape`, letting it fall
/// through toward `actions::dispatch`'s global registry instead of
/// staying local to `show_overlay`'s own in-overlay dismissal check.
fn splitter_claims() -> Vec<ChordPattern> {
    vec![
        plain(key("Left")),
        plain(key("Right")),
        plain(key("Home")),
        plain(key("End")),
        plain(key("Escape")),
        plain(key("Tab")),
        shift(key("Tab")),
    ]
}

/// D2/D3/R6/R7: draw the dock's left-edge resize splitter and return this
/// frame's effective dock width. Drawn on the *outer* Now Playing content
/// `Ui` — not inside `Panel::right`'s own content closure — for two
/// reasons: `Panel::right(..).exact_size(..)` needs the width *before* the
/// panel itself lays out (so a drag is reflected the same frame it
/// happens, SC-003), and contract D3 requires the splitter to land first
/// in this frame's Tab order, ahead of the first panel's own header.
///
/// Drag: pointer delta subtracted from the width, live every frame,
/// clamped to `[DOCK_WIDTH_MIN, max_eff]`, held in temp memory under
/// [`splitter_id`] while dragging; committed once via
/// `controller.set_dock_width` on `drag_stopped()` (data-model.md §4).
/// Keyboard (while focused): `←` grows by [`layout::DOCK_KEY_STEP`], `→`
/// shrinks by the same amount, `Home` → `DOCK_WIDTH_MIN`, `End` →
/// `max_eff`; each press commits immediately (contract D3). AccessKit:
/// role `Splitter`, label `tr("plugin-dock-resize")`, numeric
/// value/min/max and a value-text mirror.
fn show_dock_splitter<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    content_rect: Rect,
    stored: f32,
) -> f32 {
    /// The splitter's hit rect width (contract D3: "6-pt hit rect").
    const HIT_WIDTH: f32 = 6.0;

    // data-model.md §4's second clamp bound, computed the same way
    // `layout::effective_dock_width` computes it internally (that
    // function's own return value, already clamped into this bound, can't
    // hand the bound itself back out) — needed here for the splitter's own
    // Home/End targets and its AccessKit min/max.
    #[allow(clippy::manual_clamp)]
    let max_eff = (content_rect.width() - layout::HOST_CONTENT_FLOOR)
        .min(DOCK_WIDTH_MAX)
        .max(DOCK_WIDTH_MIN);

    let id = splitter_id();
    let dragging: Option<f32> = ui.memory(|m| m.data.get_temp(id));
    let base = dragging.unwrap_or(stored).clamp(DOCK_WIDTH_MIN, max_eff);

    let hit_rect = Rect::from_min_size(
        egui::pos2(
            content_rect.right() - base - HIT_WIDTH / 2.0,
            content_rect.top(),
        ),
        Vec2::new(HIT_WIDTH, content_rect.height()),
    );
    let response = ui
        .interact(hit_rect, id, Sense::click_and_drag())
        .on_hover_and_drag_cursor(CursorIcon::ResizeHorizontal);
    actions::register_claim(ui.ctx(), id, Claim::Keys(splitter_claims()));

    let mut width = base;
    if response.dragged() {
        let delta_x = ui.input(|i| i.pointer.delta().x);
        width = (width - delta_x).clamp(DOCK_WIDTH_MIN, max_eff);
        ui.memory_mut(|m| m.data.insert_temp(id, width));
    }
    if response.drag_stopped() {
        controller.set_dock_width(width);
        ui.memory_mut(|m| m.data.remove::<f32>(id));
    }

    if response.has_focus() {
        // Keep ←/→ from also driving egui's own arrow-key focus navigation,
        // which would otherwise move focus off the splitter after the
        // first step whenever a widget sits to its left or right.
        ui.memory_mut(|m| {
            m.set_focus_lock_filter(
                id,
                egui::EventFilter {
                    horizontal_arrows: true,
                    ..Default::default()
                },
            );
        });
        let stepped = if ui.input(|i| i.key_pressed(Key::Home)) {
            Some(DOCK_WIDTH_MIN)
        } else if ui.input(|i| i.key_pressed(Key::End)) {
            Some(max_eff)
        } else if ui.input(|i| i.key_pressed(Key::ArrowLeft)) {
            Some((stored + layout::DOCK_KEY_STEP).clamp(DOCK_WIDTH_MIN, max_eff))
        } else if ui.input(|i| i.key_pressed(Key::ArrowRight)) {
            Some((stored - layout::DOCK_KEY_STEP).clamp(DOCK_WIDTH_MIN, max_eff))
        } else {
            None
        };
        if let Some(new_width) = stepped {
            controller.set_dock_width(new_width);
            width = new_width;
        }
    }

    let value_text = tr_args(
        "plugin-dock-resize-value",
        &[("width", format!("{}", width.round() as i64))],
    );
    ui.ctx().accesskit_node_builder(response.id, |b| {
        b.set_role(accesskit::Role::Splitter);
        b.set_label(tr("plugin-dock-resize"));
        b.set_numeric_value(f64::from(width));
        b.set_min_numeric_value(f64::from(DOCK_WIDTH_MIN));
        b.set_max_numeric_value(f64::from(max_eff));
        b.set_value(value_text);
    });

    width
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
            }
        }
        PanelBody::Placeholder { cause } => show_placeholder(ui, controller, panel, *cause),
    }
}

/// L5/D7: plugin icon/generic glyph · truncated title (tooltip on hover
/// R9: a button's rendered width — its label's own galley width (measured
/// with `TextWrapMode::Extend`, so it never elides) plus its horizontal
/// padding on both sides. Used only to decide the header's one-row/
/// two-row layout, never to draw anything.
fn measure_button_width(ui: &Ui, text: &str) -> f32 {
    let galley = egui::WidgetText::from(text).into_galley(
        ui,
        Some(TextWrapMode::Extend),
        f32::INFINITY,
        egui::TextStyle::Button,
    );
    galley.size().x + 2.0 * ui.spacing().button_padding.x
}

fn show_header_icon<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    panel: &PanelView,
) {
    match controller.plugin_assets(panel.plugin) {
        Some(assets) if panel.has_icon => {
            plugin_assets::show_icon(ui, panel.plugin, assets, ICON_SIZE);
        }
        _ => plugin_assets::generic_glyph(ui, ICON_SIZE),
    }
}

/// D7: the title truncates to a single line. `Label::truncate()` already
/// shows the full text on hover whenever it actually elides (egui's own
/// `show_tooltip_when_elided`, on by default); the explicit
/// `show_tooltip_ui` call below covers the keyboard-focus case that
/// egui's built-in elision tooltip does not. The AccessKit label stays
/// pinned to the full, un-truncated text (FR-010), mirroring the previous
/// single-row header's own pin.
fn show_header_title(ui: &mut Ui, header_text: &str) {
    let response = ui.add(
        Label::new(theme::section_label(header_text))
            .truncate()
            .sense(Sense::focusable_noninteractive()),
    );
    if response.has_focus() {
        response.show_tooltip_ui(|ui| {
            ui.label(header_text);
        });
    }
    ui.ctx().accesskit_node_builder(response.id, |b| {
        b.set_label(header_text.to_string());
    });
}

/// The `[Float|Dock] [Close] ‹gap› [Disable]` control group, drawn either
/// on the header's single row or its own wrapped row (`show_header`
/// decides which). `float_label`/`close_label` come pre-resolved from the
/// caller (already needed there to measure the row); the Disable control's
/// own locale key is resolved inline here instead, immediately after the
/// destructive gap — contract control-variants.md B10/FR-003 (enforced by
/// `control_variants.rs`) expects that pairing written literally at the
/// one spot the destructive control is actually drawn.
fn show_header_buttons<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    panel: &PanelView,
    float_label: &str,
    close_label: &str,
) {
    if ui
        .add(Button::new(float_label).wrap_mode(TextWrapMode::Extend))
        .clicked()
    {
        let mut persisted = panel.geometry.unwrap_or_default();
        persisted.placement = match panel.placement {
            PanelPlacement::Docked => PanelPlacement::Floated,
            PanelPlacement::Floated => PanelPlacement::Docked,
        };
        controller.plugin_panel_set_placement(&panel.key, persisted);
    }
    if ui
        .add(Button::new(close_label).wrap_mode(TextWrapMode::Extend))
        .clicked()
    {
        controller.plugin_panel_close(&panel.key);
    }
    destructive_gap(ui);
    if button(ui, Variant::Destructive, tr("plugin-panel-disable")).clicked() {
        controller.plugin_panel_set_disabled(&panel.key, true);
    }
}

/// L5/D7: plugin icon/generic glyph · truncated title (tooltip on hover
/// and keyboard focus, AccessKit label pinned to the full text), then the
/// `[Float|Dock] [Close] ‹gap› [Disable]` controls — one row when the
/// title has room alongside them, otherwise a title row plus a
/// `horizontal_wrapped` button row (018-window-sizing-and-responsive-dock,
/// contract D7, research R9). Buttons are measured and drawn with
/// `TextWrapMode::Extend` so they are never themselves elided; only the
/// title ever truncates.
fn show_header<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    panel: &PanelView,
) {
    // 014-design-tokens-and-type-scale (US2, T034): the chrome header is
    // this panel's own `section`-role heading (data-model.md §6's "Panel/
    // group headers" — the plugin-drawn body below it is out of scope,
    // A9). The accessible name is pinned back to the exact, un-uppercased
    // title (FR-019), mirroring `markers::panel`'s T030.
    let header_text = tr_args(
        "plugin-panel-header",
        &[
            ("plugin", panel.plugin_name.clone()),
            ("title", panel.title.clone()),
        ],
    );
    let float_key = match panel.placement {
        PanelPlacement::Docked => "plugin-panel-float",
        PanelPlacement::Floated => "plugin-panel-dock",
    };
    let float_label = tr(float_key);
    let close_label = tr("plugin-panel-close");

    // Measured, never drawn from here — `show_header_buttons` resolves
    // Disable's own label itself, immediately before drawing it.
    let spacing = ui.spacing().item_spacing.x;
    let buttons_w = measure_button_width(ui, &float_label)
        + spacing
        + measure_button_width(ui, &close_label)
        + spacing
        + theme::controls::DESTRUCTIVE_GAP
        + spacing
        + measure_button_width(ui, &tr("plugin-panel-disable"));
    let title_budget = ui.available_width() - ICON_SIZE - spacing - buttons_w;

    if title_budget >= TITLE_MIN_WIDTH {
        ui.horizontal(|ui| {
            show_header_icon(ui, controller, panel);
            // Cap the title at its budget: left alone, `truncate()` would
            // take the whole rest of the row and push the buttons past the
            // dock's edge whenever the full title is wider than the budget.
            ui.scope(|ui| {
                ui.set_max_width(title_budget);
                show_header_title(ui, &header_text);
            });
            show_header_buttons(ui, controller, panel, &float_label, &close_label);
        });
    } else {
        ui.horizontal(|ui| {
            show_header_icon(ui, controller, panel);
            show_header_title(ui, &header_text);
        });
        ui.horizontal_wrapped(|ui| {
            show_header_buttons(ui, controller, panel, &float_label, &close_label);
        });
    }
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
    // D7/FR-011a: host status text wraps rather than truncating/eliding.
    ui.add(
        Label::new(tr_args(
            "plugin-panel-suspended",
            &[
                ("plugin", panel.plugin_name.clone()),
                ("cause", tr(cause_key(cause))),
            ],
        ))
        .wrap(),
    );
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

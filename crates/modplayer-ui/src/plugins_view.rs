// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Plugins section (009-plugin-runtime-and-permissions,
//! contracts/ui-plugins.md; layout superseded by
//! 027-plugins-list-as-table, contracts/plugins-table.md): a seven-column
//! table of every discovered plugin — Name, Source, Enabled, Health,
//! Permissions, Resource use, Actions — with a fixed header whose labels
//! share the exact x-extents of every row's cells, and — deliberately —
//! no uninstall control anywhere (FR-013).
//!
//! Rows come straight from `controller.plugins_view().rows` (already
//! sorted by name, `modplayer-core`'s own job, T104); this module only
//! draws them. Column geometry is one pure value, [`PluginsColumns`],
//! computed once per frame and shared by the header and every row, so the
//! two can never drift apart. Each cell is placed in its own column
//! rectangle and truncates (with a tooltip carrying the full text) rather
//! than overflow into its neighbour.

use std::collections::HashSet;
use std::hash::Hash;

use egui::scroll_area::ScrollBarVisibility;
use egui::text::LayoutJob;
use egui::{
    Align, Color32, FontSelection, Label, Layout, Pos2, Rect, RichText, Sense, TextStyle,
    TextWrapMode, Ui, UiBuilder, WidgetInfo, WidgetText, WidgetType, pos2, vec2,
};
use modplayer_audio_io::OutputBackend;
use modplayer_audio_source::SourceHost;
use modplayer_core::plugins::PanelRowControl;
use modplayer_core::{Health, PlaybackController, PluginId, PluginRow, Source, tr, tr_args};
use modplayer_plugin_runtime::events::SuspendCause;

use crate::plugin_panels::cause_key;
use crate::section_memory::{SectionMemory, ViewKey};
use crate::theme;
use crate::theme::controls::Variant;
use crate::widgets::controls::{SwitchKind, button, destructive_gap, row_frame, switch_bare};

/// Per-session disclosure state of the Plugins table (027 data-model §4,
/// R8): which rows have their permissions list / panels list expanded.
/// Keyed by [`PluginId`] so a health or enable change never collapses a
/// row; never persisted.
///
/// ```
/// use modplayer_ui::plugins_view::PluginsViewState;
///
/// let state = PluginsViewState::default();
/// assert!(state.permissions_open.is_empty());
/// assert!(state.panels_open.is_empty());
/// ```
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PluginsViewState {
    /// Rows whose permissions disclosure is open.
    pub permissions_open: HashSet<PluginId>,
    /// Rows whose "Panels (N)" disclosure is open.
    pub panels_open: HashSet<PluginId>,
}

impl PluginsViewState {
    /// Forget every id that is no longer in `rows` (R8), so a vanished
    /// plugin cannot leave stale expansion state behind.
    ///
    /// ```
    /// use modplayer_ui::plugins_view::PluginsViewState;
    ///
    /// let mut state = PluginsViewState::default();
    /// state.prune(&[]);
    /// assert!(state.permissions_open.is_empty());
    /// ```
    pub fn prune(&mut self, rows: &[PluginRow]) {
        let keep = |id: &PluginId| rows.iter().any(|row| row.id == *id);
        self.permissions_open.retain(keep);
        self.panels_open.retain(keep);
    }
}

/// One of the table's seven columns, in display order (FR-001).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    /// Plugin name + version.
    Name,
    /// Where the plugin comes from.
    Source,
    /// The enable/disable switch.
    Enabled,
    /// Health dot, word and suspension reason.
    Health,
    /// Permission count and disclosure.
    Permissions,
    /// CPU and memory against budgets.
    Resource,
    /// Panel controls and Restart.
    Actions,
}

impl Column {
    /// Every column, left to right.
    pub const ALL: [Self; 7] = [
        Self::Name,
        Self::Source,
        Self::Enabled,
        Self::Health,
        Self::Permissions,
        Self::Resource,
        Self::Actions,
    ];

    const fn index(self) -> usize {
        self as usize
    }

    /// The Fluent key of this column's header label.
    const fn header_key(self) -> &'static str {
        match self {
            Self::Name => "plugins-col-name",
            Self::Source => "plugins-col-source",
            Self::Enabled => "plugins-col-enabled",
            Self::Health => "plugins-col-health",
            Self::Permissions => "plugins-col-permissions",
            Self::Resource => "plugins-col-resource",
            Self::Actions => "plugins-col-actions",
        }
    }
}

/// A column's horizontal extent, relative to the table's left edge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColumnExtent {
    /// Left edge.
    pub min_x: f32,
    /// Right edge.
    pub max_x: f32,
}

impl ColumnExtent {
    /// `max_x - min_x`.
    #[must_use]
    pub fn width(&self) -> f32 {
        self.max_x - self.min_x
    }
}

/// The seven column extents for one available width (027 data-model §3,
/// R2): every width is at least its minimum, spare width goes only to the
/// weighted columns, extents are contiguous with exactly `gap` between
/// them, and the last one never passes the available width.
#[derive(Debug, Clone, PartialEq)]
pub struct PluginsColumns {
    /// One extent per [`Column`], in display order.
    pub extents: [ColumnExtent; 7],
    /// The gap between adjacent extents.
    pub gap: f32,
}

impl PluginsColumns {
    /// Gap between columns.
    pub const GAP: f32 = theme::space::XS;
    /// Per-column minimum width (Name, Source, Enabled, Health,
    /// Permissions, Resource, Actions).
    pub const MIN: [f32; 7] = [104.0, 56.0, 44.0, 80.0, 48.0, 128.0, 76.0];
    /// Share of spare width each column receives.
    pub const WEIGHT: [f32; 7] = [3.0, 1.0, 0.0, 2.0, 0.0, 1.0, 1.0];
    /// The narrowest content width the host guarantees (R12): the minima
    /// plus the six gaps.
    pub const HOST_CONTENT_FLOOR: f32 = 560.0;

    /// Compute the extents for `available_width` (L1–L4). A non-finite or
    /// negative width is treated as zero.
    ///
    /// ```
    /// use modplayer_ui::plugins_view::PluginsColumns;
    ///
    /// let cols = PluginsColumns::layout(960.0);
    /// assert_eq!(cols.extents[0].min_x, 0.0);
    /// assert!(cols.extents[6].max_x <= 960.5);
    /// ```
    #[must_use]
    pub fn layout(available_width: f32) -> Self {
        let available = if available_width.is_finite() {
            available_width.max(0.0)
        } else {
            0.0
        };
        let min_total: f32 = Self::MIN.iter().sum();
        let weight_total: f32 = Self::WEIGHT.iter().sum();

        let (gap, widths): (f32, [f32; 7]) = if available >= Self::HOST_CONTENT_FLOOR {
            let extra = available - Self::HOST_CONTENT_FLOOR;
            let mut widths = Self::MIN;
            for (width, weight) in widths.iter_mut().zip(Self::WEIGHT) {
                *width += extra * weight / weight_total;
            }
            (Self::GAP, widths)
        } else {
            let gap = Self::GAP.min(available / 6.0);
            let scale = (available - 6.0 * gap).max(0.0) / min_total;
            let mut widths = Self::MIN;
            for width in &mut widths {
                *width *= scale;
            }
            (gap, widths)
        };

        let mut x = 0.0;
        let extents = widths.map(|width| {
            let extent = ColumnExtent {
                min_x: x,
                max_x: x + width,
            };
            x += width + gap;
            extent
        });
        Self { extents, gap }
    }

    /// The extent from `from`'s left edge to `to`'s right edge (the
    /// invalid-manifest span).
    #[must_use]
    pub fn span(&self, from: Column, to: Column) -> ColumnExtent {
        ColumnExtent {
            min_x: self.extents[from.index()].min_x,
            max_x: self.extents[to.index()].max_x,
        }
    }

    /// The screen rect of `extent` for a row starting at `origin`.
    fn rect(extent: ColumnExtent, origin: Pos2, height: f32) -> Rect {
        Rect::from_min_size(
            pos2(origin.x + extent.min_x, origin.y),
            vec2(extent.width(), height),
        )
    }
}

/// Place `add` in its own child ui confined to `rect`, laid out left to
/// right and vertically centred, without advancing `ui`'s cursor. Painting
/// is clipped horizontally to the column. Returns the closure's result and
/// the area the child actually used.
fn place<R>(
    ui: &mut Ui,
    rect: Rect,
    salt: impl Hash + std::fmt::Debug,
    add: impl FnOnce(&mut Ui) -> R,
) -> (R, Rect) {
    let mut child = ui.new_child(
        UiBuilder::new()
            .id_salt(salt)
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    let clip = Rect::from_min_max(
        pos2(rect.min.x - 2.0, f32::NEG_INFINITY),
        pos2(rect.max.x + 2.0, f32::INFINITY),
    );
    child.set_clip_rect(clip.intersect(ui.clip_rect()));
    let inner = add(&mut child);
    (inner, child.min_rect())
}

/// A single-line text cell in `rect` (T6): the label truncates with an
/// ellipsis instead of overflowing, carries `full` as a hover tooltip only
/// when it actually was truncated, and its accessible name is always the
/// full text. Returns the label's response and the area used.
fn truncating_cell(
    ui: &mut Ui,
    rect: Rect,
    salt: impl Hash + std::fmt::Debug,
    text: impl Into<WidgetText>,
    full: &str,
) -> (egui::Response, Rect) {
    let text: WidgetText = text.into();
    let natural = text
        .clone()
        .into_galley(
            ui,
            Some(TextWrapMode::Extend),
            f32::INFINITY,
            TextStyle::Body,
        )
        .size()
        .x;
    let truncated = natural > rect.width() + 0.5;
    place(ui, rect, salt, |ui| {
        // Our own tooltip only: egui's built-in elided-label tooltip would
        // stack a second copy under it (027 manual walk, M3).
        let response = ui.add(Label::new(text).truncate().show_tooltip_when_elided(false));
        if truncated {
            response.on_hover_text(full)
        } else {
            response
        }
    })
}

/// Draw the whole Plugins section: heading, then either the empty state
/// or the fixed header and one row per plugin (contracts/plugins-table.md
/// T1–T5). Call once per frame while `Section::Plugins` is selected; the
/// caller (`app.rs`) is responsible for the section's own 500 ms
/// live-gauge repaint cadence. The view owns its vertical `ScrollArea` and
/// records its offset under [`ViewKey::Plugins`] itself.
pub fn show<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    state: &mut PluginsViewState,
    section_memory: &mut SectionMemory,
) {
    ui.heading(tr("plugins-title"));

    let view = controller.plugins_view();
    if view.rows.is_empty() {
        ui.label(tr("plugins-empty"));
        return;
    }
    state.prune(&view.rows);

    // The always-visible vertical scroll bar takes room from the rows; the
    // header sits outside the scroll area, so both use the same reduced
    // width (R3a) and their columns line up.
    let scroll = ui.spacing().scroll;
    let table_width = (ui.available_width() - scroll.bar_width - scroll.bar_outer_margin).max(0.0);
    let columns = PluginsColumns::layout(table_width);

    show_header(ui, &columns, table_width);

    let key = ViewKey::Plugins;
    let output = section_memory
        .scroll_area(&key)
        .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible)
        .show(ui, |ui| {
            for row in &view.rows {
                show_row(ui, controller, state, row, &columns, table_width);
            }
        });
    section_memory.record(key, output.state.offset.y);
}

/// The fixed header: seven non-focusable labels in the same column
/// rectangles the rows use (T2, T3).
fn show_header(ui: &mut Ui, columns: &PluginsColumns, table_width: f32) {
    let height = ui.spacing().interact_size.y;
    let (_, strip) = ui.allocate_space(vec2(table_width, height));
    for column in Column::ALL {
        let rect = PluginsColumns::rect(columns.extents[column.index()], strip.min, height);
        let text = tr(column.header_key());
        let styled = RichText::new(&text).text_style(TextStyle::Small).strong();
        truncating_cell(ui, rect, ("plugins-header", column.index()), styled, &text);
    }
}

/// The word shown in the Health cell for `health` (T9): the Fluent key.
#[must_use]
pub const fn health_word_key(health: Health) -> &'static str {
    match health {
        Health::Ok => "plugins-health-ok",
        Health::Warning => "plugins-health-warning",
        Health::Suspended => "plugins-health-suspended",
    }
}

/// The suspension reason shown after the health word (data-model §2): the
/// shared `plugin-suspended-cause-*` sentence for a Suspended row, the
/// `plugins-suspended-reason-unknown` fallback when the cause is missing,
/// and `None` for every other health.
///
/// ```
/// use modplayer_core::Health;
/// use modplayer_ui::plugins_view::suspension_reason;
///
/// assert_eq!(suspension_reason(Some(Health::Ok), None), None);
/// assert!(suspension_reason(Some(Health::Suspended), None).is_some());
/// ```
#[must_use]
pub fn suspension_reason(health: Option<Health>, cause: Option<SuspendCause>) -> Option<String> {
    match (health, cause) {
        (Some(Health::Suspended), Some(cause)) => Some(tr(cause_key(cause))),
        (Some(Health::Suspended), None) => Some(tr("plugins-suspended-reason-unknown")),
        _ => None,
    }
}

/// One plugin's row: every cell placed in its column rectangle inside one
/// `row_frame`. An `Invalid` row (`row.invalid_reason.is_some()`) shows
/// Name/Source/an inert unchecked switch, then the invalid-manifest
/// sentence over the Health–Resource columns and nothing under Actions
/// (T16).
fn show_row<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    state: &mut PluginsViewState,
    row: &PluginRow,
    columns: &PluginsColumns,
    table_width: f32,
) {
    // FR-009, contract I6: the row's hover/pressed fill, reserved and set
    // beneath the row's own content — zero layout change.
    let row_id = ui.id().with(("plugins-row", row.id));
    row_frame(ui, row_id, |ui| {
        let origin = ui.cursor().min;
        let height = ui.spacing().interact_size.y;
        let rect_of =
            |column: Column| PluginsColumns::rect(columns.extents[column.index()], origin, height);
        let salt = |column: Column| (row.id, column.index());
        let mut bottom = origin.y + height;

        // Name + version, one truncating line (T7).
        let version = if row.invalid_reason.is_some() || row.version.is_empty() {
            tr("plugins-dash")
        } else {
            row.version.clone()
        };
        let full_name = format!("{}  {version}", row.name);
        let mut job = LayoutJob::default();
        RichText::new(&row.name).append_to(
            &mut job,
            ui.style(),
            FontSelection::Style(TextStyle::Body),
            Align::Center,
        );
        RichText::new(format!("  {version}"))
            .text_style(TextStyle::Small)
            .weak()
            .append_to(
                &mut job,
                ui.style(),
                FontSelection::Style(TextStyle::Small),
                Align::Center,
            );
        bottom = bottom.max(
            truncating_cell(
                ui,
                rect_of(Column::Name),
                salt(Column::Name),
                job,
                &full_name,
            )
            .1
            .bottom(),
        );

        // Source.
        let source = tr(source_label_key(row.source));
        bottom = bottom.max(
            truncating_cell(
                ui,
                rect_of(Column::Source),
                salt(Column::Source),
                RichText::new(&source),
                &source,
            )
            .1
            .bottom(),
        );

        // Enabled.
        bottom = bottom.max(
            place(ui, rect_of(Column::Enabled), salt(Column::Enabled), |ui| {
                show_enable_toggle(ui, controller, row);
            })
            .1
            .bottom(),
        );

        if let Some(reason) = &row.invalid_reason {
            let span = PluginsColumns::rect(
                columns.span(Column::Health, Column::Resource),
                origin,
                height,
            );
            let text = tr_args(
                "plugins-invalid-manifest",
                &[("reason", reason.to_string())],
            );
            bottom = bottom.max(
                truncating_cell(ui, span, (row.id, "invalid"), RichText::new(&text), &text)
                    .1
                    .bottom(),
            );
        } else {
            bottom = bottom.max(show_health(
                ui,
                row,
                rect_of(Column::Health),
                salt(Column::Health),
            ));

            // Permissions: count + disclosure (T10).
            bottom = bottom.max(
                place(
                    ui,
                    rect_of(Column::Permissions),
                    salt(Column::Permissions),
                    |ui| show_permissions_cell(ui, state, row),
                )
                .1
                .bottom(),
            );

            // Resource: CPU and memory against their budgets (T11, T12).
            bottom = bottom.max(
                place(
                    ui,
                    rect_of(Column::Resource),
                    salt(Column::Resource),
                    |ui| show_resource_cell(ui, row),
                )
                .1
                .bottom(),
            );

            // Actions: panel controls, then Restart (T13).
            bottom = bottom.max(
                place(ui, rect_of(Column::Actions), salt(Column::Actions), |ui| {
                    ui.horizontal_wrapped(|ui| {
                        show_panel_controls(ui, controller, state, row);
                        show_restart(ui, controller, row);
                    });
                })
                .1
                .bottom(),
            );
        }

        // Expansion area (T14): inside the same row frame, below the cells
        // line, from the Name column's left edge to the row's right edge.
        let show_permissions =
            row.invalid_reason.is_none() && state.permissions_open.contains(&row.id);
        let show_panels = row.invalid_reason.is_none()
            && row.panels.len() >= 2
            && state.panels_open.contains(&row.id);
        if show_permissions || show_panels {
            let area = Rect::from_min_max(
                pos2(
                    origin.x + columns.extents[Column::Name.index()].min_x,
                    bottom + theme::space::XS,
                ),
                pos2(origin.x + table_width, f32::INFINITY),
            );
            let mut child = ui.new_child(
                UiBuilder::new()
                    .id_salt((row.id, "expansion"))
                    .max_rect(area)
                    .layout(Layout::top_down(Align::Min)),
            );
            if show_permissions {
                show_permission_list(&mut child, row);
            }
            if show_panels {
                show_panel_list(&mut child, controller, row);
            }
            bottom = bottom.max(child.min_rect().bottom());
        }

        ui.allocate_rect(
            Rect::from_min_max(origin, pos2(origin.x + table_width, bottom)),
            Sense::hover(),
        );
    });
}

/// Accessible name and visible text of a permissions disclosure: the
/// visible text is `"{count} ▸"` (closed) / `"{count} ▾"` (open), the
/// accessible name the `plugins-permissions-show`/`-hide` sentence.
fn permissions_disclosure(row: &PluginRow, open: bool) -> (String, String) {
    let count = row.permissions.len();
    let (arrow, key) = if open {
        ('▾', "plugins-permissions-hide")
    } else {
        ('▸', "plugins-permissions-show")
    };
    let name = tr_args(
        key,
        &[("count", count.to_string()), ("plugin", row.name.clone())],
    );
    (format!("{count} {arrow}"), name)
}

/// **Permissions** cell (T10): the count in mono digits; with at least one
/// permission it is a disclosure button toggling
/// `state.permissions_open`, with none just the label `0`.
fn show_permissions_cell(ui: &mut Ui, state: &mut PluginsViewState, row: &PluginRow) {
    if row.permissions.is_empty() {
        ui.label(theme::mono_text("0"));
        return;
    }
    let open = state.permissions_open.contains(&row.id);
    let (text, name) = permissions_disclosure(row, open);
    let response = ui.button(theme::mono_text(text));
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, name.clone()));
    if response.clicked() {
        if open {
            state.permissions_open.remove(&row.id);
        } else {
            state.permissions_open.insert(row.id);
        }
    }
}

/// One wrapping label per granted permission's `permission-*` explanation,
/// in catalog order (`row.permissions` is already in that order).
fn show_permission_list(ui: &mut Ui, row: &PluginRow) {
    for permission in &row.permissions {
        ui.add(Label::new(tr(&permission.explanation_key())).wrap());
    }
}

/// **Restart** (T13): a button for a Suspended row only, visible text
/// `plugin-panel-restart`, accessible name `plugins-restart-a11y` with the
/// plugin's name; clicking calls `plugin_restart`.
fn show_restart<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    row: &PluginRow,
) {
    if row.health != Some(Health::Suspended) {
        return;
    }
    let name = tr_args("plugins-restart-a11y", &[("plugin", row.name.clone())]);
    let response = ui.button(tr("plugin-panel-restart"));
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, name.clone()));
    if response.clicked() {
        controller.plugin_restart(row.id);
    }
}

/// **Actions** panel controls (T13, T15; 011 L6): no registered panel →
/// nothing; exactly one → its Show/Hide and Enable/Disable buttons inline;
/// two or more → a `plugins-panels-count` disclosure toggling
/// `state.panels_open` (the per-panel lines live in the row's expansion
/// area, [`show_panel_list`]).
fn show_panel_controls<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    state: &mut PluginsViewState,
    row: &PluginRow,
) {
    match row.panels.as_slice() {
        [] => {}
        [panel] => show_panel_buttons(ui, controller, row, panel),
        panels => {
            let open = state.panels_open.contains(&row.id);
            let key = if open {
                "plugins-panels-hide"
            } else {
                "plugins-panels-show"
            };
            let name = tr_args(key, &[("plugin", row.name.clone())]);
            let text = tr_args(
                "plugins-panels-count",
                &[("count", panels.len().to_string())],
            );
            let response = ui.button(text);
            response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, name.clone()));
            if response.clicked() {
                if open {
                    state.panels_open.remove(&row.id);
                } else {
                    state.panels_open.insert(row.id);
                }
            }
        }
    }
}

/// The expansion area's panels list (T14): one line per panel — a
/// truncating title, then its Show/Hide and Enable/Disable buttons.
fn show_panel_list<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    row: &PluginRow,
) {
    for panel in &row.panels {
        ui.horizontal(|ui| {
            let title_width = (ui.available_width() * 0.4).max(PluginsColumns::MIN[0]);
            ui.scope(|ui| {
                ui.set_max_width(title_width);
                ui.add(Label::new(&panel.title).truncate());
            });
            show_panel_buttons(ui, controller, row, panel);
        });
    }
}

/// One panel's two buttons, in tab order: Show/Hide, then Enable/Disable.
fn show_panel_buttons<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    row: &PluginRow,
    panel: &PanelRowControl,
) {
    show_show_hide_toggle(ui, controller, row, panel);
    show_enable_disable_toggle(ui, controller, row, panel);
}

/// The accessible name of a panel button (T15): the `plugins-panel-*-a11y`
/// sentence naming both the panel and its plugin.
fn panel_button_name(key: &str, row: &PluginRow, panel: &PanelRowControl) -> String {
    tr_args(
        key,
        &[("title", panel.title.clone()), ("plugin", row.name.clone())],
    )
}

/// **Show/Hide** (L6, session-only): visible text `plugin-panel-show`/`-hide`
/// flipping with `panel.closed`, accessible name `plugins-panel-show-a11y`/
/// `-hide-a11y`. Clicking calls `plugin_panel_show`/`plugin_panel_close`
/// immediately, never persisted (FR-006).
fn show_show_hide_toggle<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    row: &PluginRow,
    panel: &PanelRowControl,
) {
    let (text, a11y) = if panel.closed {
        ("plugin-panel-show", "plugins-panel-show-a11y")
    } else {
        ("plugin-panel-hide", "plugins-panel-hide-a11y")
    };
    let name = panel_button_name(a11y, row, panel);
    let response = ui.button(tr(text));
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, name.clone()));
    if response.clicked() {
        if panel.closed {
            controller.plugin_panel_show(&panel.key);
        } else {
            controller.plugin_panel_close(&panel.key);
        }
    }
}

/// **Enable/Disable** (L6, persisted): visible text flips with
/// `panel.disabled`, accessible name `plugins-panel-enable-a11y`/
/// `-disable-a11y`. Clicking calls `plugin_panel_set_disabled` immediately,
/// written to `[plugin_panels]` (FR-006).
fn show_enable_disable_toggle<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    row: &PluginRow,
    panel: &PanelRowControl,
) {
    if panel.disabled {
        let name = panel_button_name("plugins-panel-enable-a11y", row, panel);
        let response = ui.button(tr("plugin-panel-enable"));
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, name.clone()));
        if response.clicked() {
            controller.plugin_panel_set_disabled(&panel.key, false);
        }
    } else {
        let name = panel_button_name("plugins-panel-disable-a11y", row, panel);
        destructive_gap(ui);
        let response = button(ui, Variant::Destructive, tr("plugin-panel-disable"));
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, name.clone()));
        if response.clicked() {
            controller.plugin_panel_set_disabled(&panel.key, true);
        }
    }
}

/// **Enabled** (contracts/ui-plugins.md §2): a plain `Checkbox` bound to
/// `row.enabled`; toggling calls `plugin_enable`/`plugin_disable`
/// immediately (no confirmation, FR-024) and is inert (disabled,
/// unchecked) for an `Invalid` row, which never runs. Accessible name
/// `plugins-enable-toggle` with `$plugin` (mirrors `markers.rs`'s own
/// `loop-arm`/`loop-disarm` checkbox).
fn show_enable_toggle<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    row: &PluginRow,
) {
    let mut enabled = row.enabled;
    let label = tr_args("plugins-enable-toggle", &[("plugin", row.name.clone())]);
    let response = ui
        .add_enabled_ui(row.invalid_reason.is_none(), |ui| {
            switch_bare(ui, SwitchKind::Checkbox, &mut enabled, &label)
        })
        .inner;
    if response.changed() {
        if enabled {
            controller.plugin_enable(row.id);
        } else {
            controller.plugin_disable(row.id);
        }
    }
}

/// The health-dot colour for `health` (014-design-tokens-and-type-scale,
/// U4/U5): existing threshold logic, unchanged, now resolving to a token
/// role instead of an ad-hoc `from_rgb` — a pure mapping so
/// `type_roles::health_dot_colours_come_from_roles` can pin it without
/// standing up a `Ui`.
#[must_use]
pub fn health_color(roles: &theme::Roles, health: Health) -> Color32 {
    match health {
        Health::Ok => roles.positive,
        Health::Warning => roles.warning,
        Health::Suspended => roles.danger,
    }
}

/// **Health** (T9): a decorative painted dot (health colour) and then the word
/// (`plugins-health-*`), in the Health column. The dot never carries the
/// meaning alone — the word always spells out the state (FR-008/FR-022).
/// Returns the bottom of the area used.
fn show_health(
    ui: &mut Ui,
    row: &PluginRow,
    rect: Rect,
    salt: impl Hash + std::fmt::Debug + Copy,
) -> f32 {
    let Some(health) = row.health else {
        return rect.bottom();
    };
    let roles = theme::roles(ui.visuals());
    let color = health_color(roles, health);
    let dot_width = theme::space::LG;
    let dot_rect = Rect::from_min_size(rect.min, vec2(dot_width, rect.height()));
    let word_rect = Rect::from_min_max(pos2(rect.min.x + dot_width, rect.min.y), rect.max);
    // Painted, not a `●` label: the app's fonts have no `●` glyph and the
    // label drew as a tofu box (027 manual walk, M1).
    let (_, dot_used) = place(ui, dot_rect, (salt, "dot"), |ui| {
        let dot = theme::space::SM;
        let line = ui.text_style_height(&TextStyle::Body);
        let (rect, _) = ui.allocate_exact_size(vec2(dot, line), Sense::hover());
        ui.painter().circle_filled(rect.center(), dot / 2.0, color);
    });
    let word = tr(health_word_key(health));
    let (_, word_used) =
        truncating_cell(ui, word_rect, (salt, "word"), RichText::new(&word), &word);
    let mut bottom = dot_used.bottom().max(word_used.bottom());

    // Suspended rows: the reason follows the word on the same line, in the
    // room the word leaves, truncating with a full-text tooltip (T9).
    if let Some(reason) = suspension_reason(row.health, row.suspend_cause) {
        let word_width = WidgetText::from(RichText::new(&word))
            .into_galley(
                ui,
                Some(TextWrapMode::Extend),
                f32::INFINITY,
                TextStyle::Body,
            )
            .size()
            .x;
        let reason_min_x = (word_rect.min.x + word_width + theme::space::XS).min(rect.max.x);
        let reason_rect = Rect::from_min_max(pos2(reason_min_x, rect.min.y), rect.max);
        let (_, used) = truncating_cell(
            ui,
            reason_rect,
            (salt, "reason"),
            RichText::new(&reason).weak(),
            &reason,
        );
        bottom = bottom.max(used.bottom());
    }
    bottom
}

/// One line of the Resource cell: its text and whether the figure is at or
/// over its budget (T11, T12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceLine {
    /// The `plugins-resource-*` sentence (without the over-budget suffix).
    pub text: String,
    /// `true` when the figure is at or over its budget.
    pub over_budget: bool,
}

/// **CPU** line: `plugins-resource-cpu` with the used percentage of the
/// window (`pct_of_share × budget_pct / 100`, 1 dp) against the budget
/// (0 dp), or `plugins-resource-cpu-none` while `None` (not `Active`).
/// Over budget once the plugin has used its whole share.
///
/// ```
/// use modplayer_ui::plugins_view::cpu_line;
///
/// assert!(!cpu_line(Some(50.0), 10.0).over_budget);
/// assert!(cpu_line(Some(100.0), 10.0).over_budget);
/// ```
#[must_use]
pub fn cpu_line(pct_of_share: Option<f32>, budget_pct: f32) -> ResourceLine {
    pct_of_share.map_or_else(
        || ResourceLine {
            text: tr("plugins-resource-cpu-none"),
            over_budget: false,
        },
        |pct| ResourceLine {
            text: tr_args(
                "plugins-resource-cpu",
                &[
                    ("used", format!("{:.1}", pct * budget_pct / 100.0)),
                    ("budget", format!("{budget_pct:.0}")),
                ],
            ),
            over_budget: pct >= 100.0,
        },
    )
}

/// **Memory** line: `plugins-resource-memory` with the used MiB (1 dp)
/// against the budget in whole MiB, or `plugins-resource-memory-none`
/// while `None`. Over budget at or above the cap.
///
/// ```
/// use modplayer_ui::plugins_view::memory_line;
///
/// assert!(memory_line(Some(64 << 20), 64 << 20).over_budget);
/// assert!(!memory_line(Some(1 << 20), 64 << 20).over_budget);
/// ```
#[must_use]
pub fn memory_line(memory_bytes: Option<u64>, budget_bytes: u64) -> ResourceLine {
    const MIB: f64 = 1_048_576.0;
    memory_bytes.map_or_else(
        || ResourceLine {
            text: tr("plugins-resource-memory-none"),
            over_budget: false,
        },
        |bytes| ResourceLine {
            text: tr_args(
                "plugins-resource-memory",
                &[
                    ("used", format!("{:.1}", bytes as f64 / MIB)),
                    ("budget", format!("{:.0}", budget_bytes as f64 / MIB)),
                ],
            ),
            over_budget: bytes >= budget_bytes,
        },
    )
}

/// **Resource** cell (T11, T12): the CPU and memory lines, right-aligned in
/// mono digits (014 US3: numeric readouts compared row to row). An
/// over-budget line appends `plugins-over-budget` and takes the warning
/// role colour; a line too long for the column truncates with a tooltip.
fn show_resource_cell(ui: &mut Ui, row: &PluginRow) {
    let lines = [
        cpu_line(row.cpu_pct_of_share, row.cpu_budget_pct),
        memory_line(row.memory_bytes, row.memory_budget_bytes),
    ];
    let warning = theme::roles(ui.visuals()).warning;
    ui.with_layout(Layout::top_down(Align::Max), |ui| {
        for line in lines {
            let full = if line.over_budget {
                format!("{} {}", line.text, tr("plugins-over-budget"))
            } else {
                line.text
            };
            let mut text = theme::mono_text(&full);
            if line.over_budget {
                text = text.color(warning);
            }
            let natural = WidgetText::from(text.clone())
                .into_galley(
                    ui,
                    Some(TextWrapMode::Extend),
                    f32::INFINITY,
                    TextStyle::Body,
                )
                .size()
                .x;
            let response = ui.add(Label::new(text).truncate().show_tooltip_when_elided(false));
            if natural > ui.available_width() + 0.5 {
                response.on_hover_text(&full);
            }
        }
    });
}

/// `plugins-source-bundled` (data-model.md §3.1: `Bundled` is the only
/// reachable source this slice).
const fn source_label_key(source: Source) -> &'static str {
    match source {
        Source::Bundled => "plugins-source-bundled",
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const TOLERANCE: f32 = 0.5;

    fn assert_contiguous(cols: &PluginsColumns, available: f32) {
        for pair in cols.extents.windows(2) {
            let gap = pair[1].min_x - pair[0].max_x;
            assert!(
                (gap - cols.gap).abs() <= 1e-3,
                "extents must be separated by exactly `gap`: {cols:?}"
            );
        }
        let last = cols.extents[6];
        assert!(
            last.max_x <= available + TOLERANCE,
            "last extent must end within the available width {available}: {cols:?}"
        );
    }

    /// L4 + R12: the minimum widths plus gaps are exactly the host floor.
    #[test]
    fn min_widths_sum_to_host_content_floor() {
        let sum: f32 = PluginsColumns::MIN.iter().sum::<f32>() + 6.0 * PluginsColumns::GAP;
        assert!((sum - PluginsColumns::HOST_CONTENT_FLOOR).abs() < f32::EPSILON);
    }

    /// L1: at the floor every column is exactly its minimum.
    #[test]
    fn layout_at_floor_uses_minimums() {
        let cols = PluginsColumns::layout(PluginsColumns::HOST_CONTENT_FLOOR);
        for (i, extent) in cols.extents.iter().enumerate() {
            assert!((extent.width() - PluginsColumns::MIN[i]).abs() <= 1e-3);
        }
        assert_contiguous(&cols, PluginsColumns::HOST_CONTENT_FLOOR);
    }

    /// L1: extra width goes only to weighted columns, in proportion.
    #[test]
    fn layout_distributes_extra_by_weight() {
        let cols = PluginsColumns::layout(960.0);
        let extra = 960.0 - PluginsColumns::HOST_CONTENT_FLOOR;
        let total: f32 = PluginsColumns::WEIGHT.iter().sum();
        for (i, extent) in cols.extents.iter().enumerate() {
            let expect = PluginsColumns::MIN[i] + extra * PluginsColumns::WEIGHT[i] / total;
            assert!((extent.width() - expect).abs() <= 1e-2, "column {i}");
        }
        assert_contiguous(&cols, 960.0);
    }

    /// L2: a defensive sub-floor width scales the minimums down.
    #[test]
    fn layout_below_floor_shrinks_and_fits() {
        let cols = PluginsColumns::layout(300.0);
        assert_contiguous(&cols, 300.0);
        assert!(cols.extents.iter().all(|e| e.width() >= 0.0));
    }

    /// Zero, negative and NaN widths never panic or go negative.
    #[test]
    fn layout_degenerate_widths_are_safe() {
        for w in [0.0, -10.0, f32::NAN, 1.0] {
            let cols = PluginsColumns::layout(w);
            assert!(cols.extents.iter().all(|e| e.width() >= 0.0), "w = {w}");
            assert!(cols.extents[6].max_x <= w.max(0.0) + TOLERANCE || w.is_nan());
        }
    }

    #[test]
    fn span_covers_from_min_to_max() {
        let cols = PluginsColumns::layout(960.0);
        let span = cols.span(Column::Health, Column::Resource);
        assert!((span.min_x - cols.extents[3].min_x).abs() < f32::EPSILON);
        assert!((span.max_x - cols.extents[5].max_x).abs() < f32::EPSILON);
    }

    /// T9: each health maps to its Fluent key.
    #[test]
    fn health_word_keys() {
        assert_eq!(health_word_key(Health::Ok), "plugins-health-ok");
        assert_eq!(health_word_key(Health::Warning), "plugins-health-warning");
        assert_eq!(
            health_word_key(Health::Suspended),
            "plugins-health-suspended"
        );
    }

    /// data-model §2: the reason exists only for a Suspended row.
    #[test]
    fn suspension_reason_per_cause() {
        use modplayer_plugin_runtime::events::SuspendCause;

        let cases = [
            (SuspendCause::Hang, "plugin-suspended-cause-hang"),
            (SuspendCause::CpuShare, "plugin-suspended-cause-cpu-share"),
            (SuspendCause::Memory, "plugin-suspended-cause-memory"),
            (
                SuspendCause::DidNotStart,
                "plugin-suspended-cause-did-not-start",
            ),
        ];
        for (cause, key) in cases {
            assert_eq!(
                suspension_reason(Some(Health::Suspended), Some(cause)),
                Some(tr(key))
            );
        }
        assert_eq!(
            suspension_reason(Some(Health::Suspended), None),
            Some(tr("plugins-suspended-reason-unknown"))
        );
        for health in [None, Some(Health::Ok), Some(Health::Warning)] {
            assert_eq!(suspension_reason(health, None), None);
            assert_eq!(suspension_reason(health, Some(SuspendCause::Hang)), None);
        }
    }

    /// Fluent wraps placeables in bidi isolates; strip them to compare.
    fn plain(text: &str) -> String {
        text.replace(['\u{2068}', '\u{2069}'], "")
    }

    /// T11: CPU line = used (share-fraction × budget, 1 dp) / budget (0 dp).
    #[test]
    fn cpu_line_shows_used_against_budget() {
        let line = cpu_line(Some(12.0), 10.0);
        assert_eq!(plain(&line.text), "CPU 1.2 % / 10 %");
        assert!(!line.over_budget);
    }

    /// T12: a full share (100 % of it) is over budget.
    #[test]
    fn cpu_line_over_budget_at_full_share() {
        let line = cpu_line(Some(100.0), 10.0);
        assert_eq!(plain(&line.text), "CPU 10.0 % / 10 %");
        assert!(line.over_budget);
    }

    /// T11: memory in MiB, one decimal; budget whole MiB.
    #[test]
    fn memory_line_shows_used_against_budget() {
        let used = 3 * 1_048_576 + 1_048_576 / 5; // 3.2 MiB
        let line = memory_line(Some(used), 64 * 1_048_576);
        assert_eq!(plain(&line.text), "Mem 3.2 MB / 64 MB");
        assert!(!line.over_budget);
    }

    /// T12: memory at or above the budget is over budget.
    #[test]
    fn memory_line_over_budget_at_cap() {
        let line = memory_line(Some(64 * 1_048_576), 64 * 1_048_576);
        assert!(line.over_budget);
        assert!(memory_line(Some(65 * 1_048_576), 64 * 1_048_576).over_budget);
    }

    /// T11: not Active → the dash strings, never over budget.
    #[test]
    fn resource_lines_none_are_dashes() {
        let cpu = cpu_line(None, 10.0);
        assert_eq!(cpu.text, tr("plugins-resource-cpu-none"));
        assert!(!cpu.over_budget);
        let memory = memory_line(None, 64 * 1_048_576);
        assert_eq!(memory.text, tr("plugins-resource-memory-none"));
        assert!(!memory.over_budget);
    }

    proptest! {
        /// L1, L3, L4 for every width at or above the floor.
        #[test]
        fn layout_fits_at_or_above_floor(w in 560.0f32..4000.0) {
            let cols = PluginsColumns::layout(w);
            let total: f32 = cols.extents.iter().map(ColumnExtent::width).sum::<f32>()
                + 6.0 * cols.gap;
            prop_assert!(total <= w + TOLERANCE);
            for (i, extent) in cols.extents.iter().enumerate() {
                prop_assert!(extent.width() >= PluginsColumns::MIN[i] - 1e-3);
            }
            assert_contiguous(&cols, w);
        }

        /// L2, L3, L4 for widths below the floor.
        #[test]
        fn layout_fits_below_floor(w in 0.0f32..560.0) {
            let cols = PluginsColumns::layout(w);
            prop_assert!(cols.extents.iter().all(|e| e.width() >= 0.0));
            assert_contiguous(&cols, w);
        }
    }
}

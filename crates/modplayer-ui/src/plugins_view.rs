// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Plugins section (009-plugin-runtime-and-permissions,
//! contracts/ui-plugins.md): a column list of every discovered plugin
//! with live health/CPU/memory, a one-action enable/disable checkbox per
//! row, and — deliberately — no uninstall control anywhere (FR-013).
//!
//! Rows come straight from `controller.plugins_view().rows` (already
//! sorted by name, `modplayer-core`'s own job, T104); this module only
//! draws them. The whole table is a plain vertical stack of
//! `ui.horizontal` rows rather than a custom grid widget, so egui's
//! default Tab order and AccessKit tree apply with no extra plumbing
//! (Constitution X).

use egui::{Color32, RichText, Ui};
use modplayer_audio_io::OutputBackend;
use modplayer_audio_source::SourceHost;
use modplayer_core::plugins::PanelRowControl;
use modplayer_core::{Health, PlaybackController, PluginRow, Source, tr, tr_args};

use crate::theme;

/// Draw the whole Plugins section: heading, either the empty state or one
/// row per plugin (contracts/ui-plugins.md §2). Call once per frame while
/// `Section::Plugins` is selected; the caller (`app.rs`, T106) is
/// responsible for the section's own 500 ms live-gauge repaint cadence.
pub fn show<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
) {
    ui.heading(tr("plugins-title"));

    let view = controller.plugins_view();
    if view.rows.is_empty() {
        ui.label(tr("plugins-empty"));
        return;
    }

    show_header(ui);
    for row in &view.rows {
        show_row(ui, controller, row);
    }
}

/// The column header labels (contracts/ui-plugins.md §2), in the fixed
/// display order — decorative only, not part of any row's own accessible
/// name.
fn show_header(ui: &mut Ui) {
    ui.horizontal(|ui| {
        for key in [
            "plugins-col-name",
            "plugins-col-version",
            "plugins-col-source",
            "plugins-col-enabled",
            "plugins-col-health",
            "plugins-col-permissions",
            "plugins-col-cpu",
            "plugins-col-memory",
        ] {
            ui.label(tr(key));
        }
    });
}

/// One plugin's row. An `Invalid` row (`row.invalid_reason.is_some()`)
/// renders name/version/source/an inert unchecked toggle, then the
/// invalid-manifest sentence spanning the remaining columns in place of
/// health/permissions/CPU/memory (contracts/ui-plugins.md §2's example
/// table).
fn show_row<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    row: &PluginRow,
) {
    ui.horizontal(|ui| {
        ui.label(&row.name);
        ui.label(&row.version);
        ui.label(tr(source_label_key(row.source)));

        show_enable_toggle(ui, controller, row);

        if let Some(reason) = &row.invalid_reason {
            ui.label(tr_args(
                "plugins-invalid-manifest",
                &[("reason", reason.to_string())],
            ));
            return;
        }

        if let Some(health) = row.health {
            show_health(ui, health);
        }

        ui.label(permissions_summary(row));
        // 014-design-tokens-and-type-scale (US3, T040): CPU/memory figures
        // are numeric readouts compared row-to-row — `mono` so their digits
        // share one advance width and line up in a fixed-width column.
        ui.label(theme::mono_text(cpu_label(row.cpu_pct_of_share)));
        ui.label(theme::mono_text(memory_label(row.memory_bytes)));
    });

    show_panel_controls(ui, controller, row);
}

/// L6 (011-plugin-ui-contributions, contracts/ui-panels.md): one indented
/// line per panel this plugin currently has registered, each with its own
/// session-only Show/Hide toggle and persisted Enable/Disable toggle.
/// Nothing renders for a plugin with no registered panels — the common
/// case for every fixture that never calls `register_panel`.
fn show_panel_controls<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    row: &PluginRow,
) {
    for panel in &row.panels {
        ui.horizontal(|ui| {
            ui.add_space(theme::space::LG);
            ui.label(&panel.title);
            show_show_hide_toggle(ui, controller, panel);
            show_enable_disable_toggle(ui, controller, panel);
        });
    }
}

/// **Show/Hide** (L6, session-only): a button whose label flips with
/// `panel.closed` — the preceding `ui.label(&panel.title)` (see
/// `show_panel_controls`) is this button's own accessible context, so the
/// button itself stays the same short `plugin-panel-show`/`-hide` text
/// contracts/ui-panels.md §4 names. Clicking calls `plugin_panel_show`/
/// `plugin_panel_close` immediately, never persisted (FR-006).
fn show_show_hide_toggle<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    panel: &PanelRowControl,
) {
    let key = if panel.closed {
        "plugin-panel-show"
    } else {
        "plugin-panel-hide"
    };
    if ui.button(tr(key)).clicked() {
        if panel.closed {
            controller.plugin_panel_show(&panel.key);
        } else {
            controller.plugin_panel_close(&panel.key);
        }
    }
}

/// **Enable/Disable** (L6, persisted): a button whose label flips with
/// `panel.disabled`. Clicking calls `plugin_panel_set_disabled`
/// immediately, written to `[plugin_panels]` (FR-006).
fn show_enable_disable_toggle<B: OutputBackend, H: SourceHost>(
    ui: &mut Ui,
    controller: &mut PlaybackController<B, H>,
    panel: &PanelRowControl,
) {
    let key = if panel.disabled {
        "plugin-panel-enable"
    } else {
        "plugin-panel-disable"
    };
    if ui.button(tr(key)).clicked() {
        controller.plugin_panel_set_disabled(&panel.key, !panel.disabled);
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
    let response = ui.add_enabled(
        row.invalid_reason.is_none(),
        egui::Checkbox::new(&mut enabled, label),
    );
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

/// **Health** (contracts/ui-plugins.md §2): a `plugins-health-*` label
/// with a colored dot ahead of it. The dot is purely decorative — the
/// label text alone always spells out the state (Constitution/FR-022:
/// colour never carries meaning alone).
fn show_health(ui: &mut Ui, health: Health) {
    let roles = theme::roles(ui.visuals());
    let color = health_color(roles, health);
    let key = match health {
        Health::Ok => "plugins-health-ok",
        Health::Warning => "plugins-health-warning",
        Health::Suspended => "plugins-health-suspended",
    };
    ui.horizontal(|ui| {
        ui.label(RichText::new("●").color(color));
        ui.label(tr(key));
    });
}

/// **Permissions** (contracts/ui-plugins.md §2): the granted permissions'
/// `permission-*` explanation strings, in catalog order (already
/// guaranteed by `Grants::granted()`/`PluginsView::from_records`), joined
/// with `plugins-list-separator`.
fn permissions_summary(row: &PluginRow) -> String {
    row.permissions
        .iter()
        .map(|permission| tr(&permission.explanation_key()))
        .collect::<Vec<_>>()
        .join(&tr("plugins-list-separator"))
}

/// **CPU** (contracts/ui-plugins.md §2): `plugins-cpu` as a whole-percent
/// figure of the 10 % aggregate share, or `plugins-dash` while `None`
/// (not `Active`).
fn cpu_label(cpu_pct_of_share: Option<f32>) -> String {
    cpu_pct_of_share.map_or_else(
        || tr("plugins-dash"),
        |pct| tr_args("plugins-cpu", &[("pct", format!("{pct:.0}"))]),
    )
}

/// **Memory** (contracts/ui-plugins.md §2): `plugins-memory` as
/// "<used> MB / 64 MB" (one decimal), or `plugins-dash` while `None` (not
/// `Active`).
fn memory_label(memory_bytes: Option<u64>) -> String {
    memory_bytes.map_or_else(
        || tr("plugins-dash"),
        |bytes| {
            let mb = bytes as f64 / (1024.0 * 1024.0);
            tr_args("plugins-memory", &[("used", format!("{mb:.1}"))])
        },
    )
}

/// `plugins-source-bundled` (data-model.md §3.1: `Bundled` is the only
/// reachable source this slice).
const fn source_label_key(source: Source) -> &'static str {
    match source {
        Source::Bundled => "plugins-source-bundled",
    }
}

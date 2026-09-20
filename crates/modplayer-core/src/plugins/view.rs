// SPDX-License-Identifier: MIT OR Apache-2.0

//! `PluginsView`/`PluginRow` (FR-023, data-model.md §3.3): the real
//! `PluginRecord` → row mapping (US4 T104, contracts/ui-plugins.md §2).

use std::time::Instant;

use modplayer_capability_gateway::api::Permission;
use modplayer_capability_gateway::manifest::{ManifestError, PluginIdentifier};
use modplayer_capability_gateway::ui::OverlayPrimitive;
use modplayer_plugin_runtime::events::SuspendCause;

use crate::i18n::tr;
use crate::settings::PanelPersisted;

use super::focus::{FocusArbiter, FocusHolder, FocusPolicy};
use super::ui::assets::PluginAssets;
use super::ui::overlay::OverlayRegistry;
use super::ui::panel::{PanelKey, PanelRegistry, WidgetState};
use super::ui::settings::{SettingsPage, SettingsRegistry};
use super::{Health, Lifecycle, PluginId, PluginRecord, Source};

/// One row of the Plugins list (FR-023).
#[derive(Debug, Clone, PartialEq)]
pub struct PluginRow {
    pub id: PluginId,
    pub identifier: PluginIdentifier,
    pub name: String,
    pub version: String,
    pub source: Source,
    pub enabled: bool,
    pub health: Option<Health>,
    pub invalid_reason: Option<ManifestError>,
    /// Granted permissions, in catalog order.
    pub permissions: Vec<Permission>,
    /// `None` unless `Active` (FR-023: rendered as "—" otherwise).
    pub cpu_pct_of_share: Option<f32>,
    pub memory_bytes: Option<u64>,
    /// Always `false` (FR-013: no uninstall control exists).
    pub can_uninstall: bool,
    /// 011-plugin-ui-contributions (FR-006, L6): one row per panel this
    /// plugin currently has registered — the Plugins-list row's own
    /// Show/Hide + Enable/Disable controls.
    pub panels: Vec<PanelRowControl>,
}

/// One of [`PluginRow::panels`] (FR-006, contracts/ui-panels.md L6).
#[derive(Debug, Clone, PartialEq)]
pub struct PanelRowControl {
    pub key: PanelKey,
    pub title: String,
    /// Session-only: `true` while closed (`PanelRegistry::is_closed`).
    pub closed: bool,
    /// Persisted: `true` while disabled in `[plugin_panels]`.
    pub disabled: bool,
}

/// The whole Plugins list, sorted by `name` case-insensitively (FR-023).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PluginsView {
    pub rows: Vec<PluginRow>,
}

impl PluginsView {
    /// Build the list from every discovered [`PluginRecord`]
    /// (contracts/ui-plugins.md §2): one row per record, sorted by
    /// display name case-insensitively (`record`s are already discovered
    /// in that order, but this re-sorts defensively rather than assuming
    /// it). `now` derives a row's [`Health`] the same way
    /// `PluginRecord::derive_health` does — a `Warning` window that has
    /// elapsed since the last runtime event decays to `Ok` on the next
    /// call, exactly the live behaviour the section's 500 ms repaint
    /// (T106) exists to show. `panels`/`plugin_panels` feed each row's
    /// FR-006 panel controls (011-plugin-ui-contributions, T047).
    #[must_use]
    pub fn from_records(
        records: &[PluginRecord],
        now: Instant,
        panels: &PanelRegistry,
        plugin_panels: &std::collections::BTreeMap<String, PanelPersisted>,
    ) -> Self {
        let mut rows: Vec<PluginRow> = records
            .iter()
            .map(|record| row(record, now, panels, plugin_panels))
            .collect();
        rows.sort_by_key(|row| row.name.to_lowercase());
        Self { rows }
    }
}

/// One record's row (contracts/ui-plugins.md §2): CPU/memory are `None`
/// unless the plugin is `Active` (FR-023: rendered as "—" by the UI, not
/// here — this model layer only ever hands the UI real values or
/// `None`).
fn row(
    record: &PluginRecord,
    now: Instant,
    panels: &PanelRegistry,
    plugin_panels: &std::collections::BTreeMap<String, PanelPersisted>,
) -> PluginRow {
    let name = record
        .manifest
        .as_ref()
        .map(|m| m.name.clone())
        .unwrap_or_else(|_| record.identifier.as_str().to_string());
    let version = record
        .manifest
        .as_ref()
        .map(|m| m.version.to_string())
        .unwrap_or_else(|_| tr("plugins-dash"));
    let active = matches!(record.lifecycle, Lifecycle::Active);
    let cpu_pct_of_share = active
        .then_some(record.gauges.as_ref())
        .flatten()
        .map(|gauges| f32::from(gauges.cpu_permille_of_share()) / 10.0);
    let memory_bytes = active
        .then_some(record.gauges.as_ref())
        .flatten()
        .map(|gauges| gauges.used_bytes());

    let panel_rows: Vec<PanelRowControl> = panels
        .for_plugin(record.id)
        .iter()
        .map(|panel| {
            let key = PanelKey::new(record.identifier.clone(), panel.id.clone());
            let disabled = plugin_panels
                .get(&key.storage_key())
                .is_some_and(|p| p.disabled);
            PanelRowControl {
                closed: panels.is_closed(&key),
                disabled,
                title: panel.title.clone(),
                key,
            }
        })
        .collect();

    PluginRow {
        id: record.id,
        identifier: record.identifier.clone(),
        name,
        version,
        source: record.source,
        enabled: record.enabled,
        health: record.derive_health(now),
        invalid_reason: record.manifest.as_ref().err().cloned(),
        permissions: record.grants.granted().collect(),
        cpu_pct_of_share,
        memory_bytes,
        can_uninstall: false,
        panels: panel_rows,
    }
}

/// 011-plugin-ui-contributions (data-model.md §4.7, contracts/ui-panels.md
/// §2): every panel currently visible, split by placement — `docked`
/// sorted by (owning plugin name, that plugin's own registration `seq`,
/// FR-005); `floated` in the same order (its own on-screen position is
/// whatever `settings.toml` persisted, not this ordering).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PluginPanelsView {
    pub docked: Vec<PanelView>,
    pub floated: Vec<PanelView>,
}

/// One visible panel (data-model.md §4.7).
#[derive(Debug, Clone, PartialEq)]
pub struct PanelView {
    pub key: PanelKey,
    pub plugin: PluginId,
    pub plugin_name: String,
    pub has_icon: bool,
    pub title: String,
    pub placement: crate::settings::PanelPlacement,
    pub geometry: Option<PanelPersisted>,
    pub body: PanelBody,
}

/// A panel's content (contracts/ui-panels.md P5, FR-025): `Live` while
/// the plugin is `Active`; `Placeholder` (no widgets rendered) while
/// `Suspended`.
#[derive(Debug, Clone, PartialEq)]
pub enum PanelBody {
    Live(Vec<WidgetState>),
    Placeholder { cause: SuspendCause },
}

impl PluginPanelsView {
    /// Build the whole view (contracts/ui-panels.md §2 "Visibility"): a
    /// panel appears iff its plugin is `Active` (rendered `Live`) or
    /// `Suspended` (rendered `Placeholder`), it is not session-closed, and
    /// it is not persisted-disabled in `[plugin_panels]`.
    #[must_use]
    pub fn from_records(
        records: &[PluginRecord],
        panels: &PanelRegistry,
        plugin_panels: &std::collections::BTreeMap<String, PanelPersisted>,
    ) -> Self {
        let mut docked = Vec::new();
        let mut floated = Vec::new();
        // (name, seq) order, exactly L1's dock order — applied here so
        // `floated` (whose own on-screen position ignores this ordering,
        // per doc comment above) still enumerates deterministically.
        let mut sorted_records: Vec<&PluginRecord> = records.iter().collect();
        sorted_records.sort_by_key(|r| {
            r.manifest
                .as_ref()
                .map(|m| m.name.to_lowercase())
                .unwrap_or_else(|_| r.identifier.as_str().to_lowercase())
        });
        for record in sorted_records {
            let cause = match &record.lifecycle {
                Lifecycle::Active => None,
                Lifecycle::Suspended { cause } => Some(*cause),
                _ => continue,
            };
            let name = record
                .manifest
                .as_ref()
                .map(|m| m.name.clone())
                .unwrap_or_else(|_| record.identifier.as_str().to_string());
            let mut plugin_panels_sorted: Vec<_> = panels.for_plugin(record.id).iter().collect();
            plugin_panels_sorted.sort_by_key(|p| p.seq);
            for panel in plugin_panels_sorted {
                let key = PanelKey::new(record.identifier.clone(), panel.id.clone());
                if panels.is_closed(&key) {
                    continue;
                }
                let persisted = plugin_panels.get(&key.storage_key()).copied();
                if persisted.is_some_and(|p| p.disabled) {
                    continue;
                }
                let placement = persisted.map_or_else(Default::default, |p| p.placement);
                let body = match cause {
                    None => PanelBody::Live(panel.widgets.clone()),
                    Some(cause) => PanelBody::Placeholder { cause },
                };
                let view = PanelView {
                    key,
                    plugin: record.id,
                    plugin_name: name.clone(),
                    has_icon: record.assets.icon.is_some(),
                    title: panel.title.clone(),
                    placement,
                    geometry: persisted,
                    body,
                };
                match placement {
                    crate::settings::PanelPlacement::Docked => docked.push(view),
                    crate::settings::PanelPlacement::Floated => floated.push(view),
                }
            }
        }
        Self { docked, floated }
    }
}

/// One row of the Transport panel (010-transport-focus, FR-008,
/// data-model.md §2.1).
#[derive(Debug, Clone, PartialEq)]
pub struct FocusRow {
    pub id: PluginId,
    pub name: String,
    pub holds: bool,
    /// 1-based among outstanding requests; `None` = not requesting.
    pub request_order: Option<usize>,
}

/// The Transport panel's whole read model (FR-008, contracts/focus-
/// arbitration.md C9): the UI never touches `PluginRecord`s or the
/// [`FocusArbiter`] directly. `holder` is `None` when the host holds
/// focus, `Some` otherwise; when `Some`, it is one of `rows` (never a
/// plugin that has since dropped out of the eligible set, C9).
#[derive(Debug, Clone, PartialEq)]
pub struct TransportFocusView {
    pub policy: FocusPolicy,
    pub holder: Option<FocusRow>,
    pub rows: Vec<FocusRow>,
}

impl TransportFocusView {
    /// Build the view (research R8, contracts/focus-arbitration.md C9):
    /// `rows` = every record that is `enabled`, whose `lifecycle` is
    /// `Loading` or `Active`, and that was granted
    /// `transport.control` — sorted by name, exactly like
    /// [`PluginsView::from_records`]. `holder`/`request_order` come from
    /// `arbiter`; a plugin that is currently the holder or pending but no
    /// longer eligible (suspended, disabled, or lost its grant somehow)
    /// never appears — the caller's own teardown (`PluginHost::stop`)
    /// already vacated it from the arbiter by the time this is read.
    #[must_use]
    pub fn from_records_and_arbiter(records: &[PluginRecord], arbiter: &FocusArbiter) -> Self {
        let mut rows: Vec<FocusRow> = records
            .iter()
            .filter(|record| {
                record.enabled
                    && matches!(record.lifecycle, Lifecycle::Loading | Lifecycle::Active)
                    && record.grants.holds(Permission::TransportControl)
            })
            .map(|record| {
                let name = record
                    .manifest
                    .as_ref()
                    .map(|m| m.name.clone())
                    .unwrap_or_else(|_| record.identifier.as_str().to_string());
                FocusRow {
                    id: record.id,
                    holds: arbiter.holder() == FocusHolder::Plugin(record.id),
                    request_order: arbiter.request_order(record.id),
                    name,
                }
            })
            .collect();
        rows.sort_by_key(|row| row.name.to_lowercase());

        let holder = match arbiter.holder() {
            FocusHolder::Host => None,
            FocusHolder::Plugin(id) => rows.iter().find(|row| row.id == id).cloned(),
        };

        Self {
            policy: arbiter.policy(),
            holder,
            rows,
        }
    }
}

/// 011-plugin-ui-contributions (data-model.md §4.3, contracts/overlays-
/// settings-notify.md O4): one `Active` plugin's overlay primitives, ready
/// for `modplayer-ui` to paint. `glyph_assets` is a cheap clone
/// (`DecodedPng::rgba` is `Arc`-backed) of the same [`PluginAssets`]
/// [`PanelView::has_icon`] already reads from the record — kept here too
/// so painting a `glyph { icon = "<package key>" }` primitive never needs
/// a second controller call.
#[derive(Debug, Clone, PartialEq)]
pub struct OverlayLayer {
    /// This plugin's own first-registration sequence (O4) — the whole
    /// cross-plugin z-order key; ties are impossible (each plugin has at
    /// most one set of overlays).
    pub plugin_seq: u64,
    pub plugin: PluginId,
    pub primitives: Vec<OverlayPrimitive>,
    pub glyph_assets: PluginAssets,
}

impl OverlayLayer {
    /// Build every visible layer (O4): one per plugin that is both
    /// `Active` and currently has at least one registered primitive,
    /// ordered by that plugin's own first-registration sequence — stable
    /// for the session regardless of how many times it has cleared and
    /// re-added since (`OverlayRegistry::clear`'s own doc note). Mirrors
    /// [`PluginPanelsView::from_records`]'s split: the registry itself
    /// never reads a [`PluginRecord`], lifecycle-aware view assembly lives
    /// here.
    #[must_use]
    pub fn from_records(records: &[PluginRecord], overlays: &OverlayRegistry) -> Vec<Self> {
        let mut layers: Vec<Self> = records
            .iter()
            .filter(|record| matches!(record.lifecycle, Lifecycle::Active))
            .filter_map(|record| {
                let plugin_seq = overlays.seq_of(record.id)?;
                let primitives = overlays.primitives_for(record.id);
                if primitives.is_empty() {
                    return None;
                }
                Some(Self {
                    plugin_seq,
                    plugin: record.id,
                    primitives,
                    glyph_assets: record.assets.clone(),
                })
            })
            .collect();
        layers.sort_by_key(|layer| layer.plugin_seq);
        layers
    }
}

/// 011-plugin-ui-contributions (data-model.md §4.7, contracts/overlays-
/// settings-notify.md S2): one `Active` plugin's settings page, ready for
/// `modplayer-ui`'s Settings › Plugins screen. `name` is this plugin's
/// resolved display name (identical to `PluginRow::name`'s own fallback),
/// kept here so the UI never needs a second lookup into `records` to
/// render the sub-page list.
#[derive(Debug, Clone, PartialEq)]
pub struct PluginSettingsView {
    pub plugin: PluginId,
    pub name: String,
    pub page: SettingsPage,
}

impl PluginSettingsView {
    /// Build every visible page (S2 "Visibility"): one per plugin that is
    /// both `Active` and has ever registered a settings page, sorted by
    /// name case-insensitively — exactly [`PluginsView::from_records`]'s
    /// own sort. A `Suspended`/`Disabled`/`Invalid` plugin's page is
    /// skipped here (hidden, not removed — [`SettingsRegistry::page_for`]
    /// still holds its values, data-model.md §8 "Settings" transitions).
    #[must_use]
    pub fn from_records(records: &[PluginRecord], settings: &SettingsRegistry) -> Vec<Self> {
        let mut views: Vec<Self> = records
            .iter()
            .filter(|record| matches!(record.lifecycle, Lifecycle::Active))
            .filter_map(|record| {
                let page = settings.page_for(record.id)?.clone();
                let name = record
                    .manifest
                    .as_ref()
                    .map(|m| m.name.clone())
                    .unwrap_or_else(|_| record.identifier.as_str().to_string());
                Some(Self {
                    plugin: record.id,
                    name,
                    page,
                })
            })
            .collect();
        views.sort_by_key(|view| view.name.to_lowercase());
        views
    }
}

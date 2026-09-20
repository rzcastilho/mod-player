// SPDX-License-Identifier: MIT OR Apache-2.0

//! `PluginsView`/`PluginRow` (FR-023, data-model.md §3.3): the real
//! `PluginRecord` → row mapping (US4 T104, contracts/ui-plugins.md §2).

use std::time::Instant;

use modplayer_capability_gateway::api::Permission;
use modplayer_capability_gateway::manifest::{ManifestError, PluginIdentifier};

use crate::i18n::tr;

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
    /// (T106) exists to show.
    #[must_use]
    pub fn from_records(records: &[PluginRecord], now: Instant) -> Self {
        let mut rows: Vec<PluginRow> = records.iter().map(|record| row(record, now)).collect();
        rows.sort_by_key(|row| row.name.to_lowercase());
        Self { rows }
    }
}

/// One record's row (contracts/ui-plugins.md §2): CPU/memory are `None`
/// unless the plugin is `Active` (FR-023: rendered as "—" by the UI, not
/// here — this model layer only ever hands the UI real values or
/// `None`).
fn row(record: &PluginRecord, now: Instant) -> PluginRow {
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
    }
}

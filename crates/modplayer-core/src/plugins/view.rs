// SPDX-License-Identifier: MIT OR Apache-2.0

//! `PluginsView`/`PluginRow` (FR-023, data-model.md §3.3): the real
//! `PluginRecord` → row mapping (US4 T104, contracts/ui-plugins.md §2).

use std::time::Instant;

use modplayer_capability_gateway::api::Permission;
use modplayer_capability_gateway::manifest::{ManifestError, PluginIdentifier};

use crate::i18n::tr;

use super::focus::{FocusArbiter, FocusHolder, FocusPolicy};
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
